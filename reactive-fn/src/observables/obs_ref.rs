use super::*;

pub fn obs_ref<S, T, F>(this: S, f: F) -> ObsRef<impl ObservableRef<Item = T>>
where
    S: 'static,
    T: 'static + ?Sized,
    F: Fn(&S, &mut dyn FnMut(&T, &BindContext), &BindContext) + 'static,
{
    struct ReRefFn<S, T: ?Sized, F> {
        this: S,
        f: F,
        _phantom: PhantomData<fn(&Self) -> &T>,
    }
    impl<S, T, F> ObservableRef for ReRefFn<S, T, F>
    where
        S: 'static,
        T: 'static + ?Sized,
        F: Fn(&S, &mut dyn FnMut(&T, &BindContext), &BindContext) + 'static,
    {
        type Item = T;

        fn with<U>(&self, f: impl FnOnce(&T, &BindContext) -> U, cx: &BindContext) -> U {
            let mut result = None;
            let mut f = Some(f);
            (self.f)(
                &self.this,
                &mut |value, cx| {
                    result = Some((f.take().unwrap())(value, cx));
                },
                cx,
            );
            result.unwrap()
        }
    }
    ObsRef(ReRefFn {
        this,
        f,
        _phantom: PhantomData,
    })
}

pub fn obs_ref_constant<T: 'static>(value: T) -> ObsRef<impl ObservableRef<Item = T>> {
    struct ReRefConstant<T>(T);
    impl<T: 'static> ObservableRef for ReRefConstant<T> {
        type Item = T;
        fn with<U>(&self, f: impl FnOnce(&Self::Item, &BindContext) -> U, cx: &BindContext) -> U {
            f(&self.0, cx)
        }
    }
    ObsRef(ReRefConstant(value))
}
pub fn obs_ref_static<T>(value: &'static T) -> ObsRef<impl ObservableRef<Item = T>> {
    struct ReRefStatic<T: 'static>(&'static T);
    impl<T: 'static> ObservableRef for ReRefStatic<T> {
        type Item = T;
        fn with<U>(&self, f: impl FnOnce(&Self::Item, &BindContext) -> U, cx: &BindContext) -> U {
            f(self.0, cx)
        }
    }
    ObsRef(ReRefStatic(value))
}

#[derive(Clone)]
pub struct ObsRef<S>(pub(super) S);

impl<S: ObservableRef> ObsRef<S> {
    pub fn with<U>(&self, f: impl FnOnce(&S::Item, &BindContext) -> U, cx: &BindContext) -> U {
        self.0.with(f, cx)
    }
    pub fn head_tail(self, f: impl FnOnce(&S::Item)) -> TailRefOps<S> {
        BindScope::with(|scope| self.head_tail_with(scope, f))
    }
    pub fn head_tail_with(self, scope: &BindScope, f: impl FnOnce(&S::Item)) -> TailRefOps<S> {
        TailRefOps::new(self.0, scope, f)
    }
    pub fn into_dyn(self) -> DynObsRef<S::Item> {
        self.0.into_dyn()
    }
    pub fn map<T: 'static>(
        self,
        f: impl Fn(&S::Item) -> T + 'static,
    ) -> Obs<impl Observable<Item = T>> {
        obs(move |cx| self.with(|x, _| f(x), cx))
    }
    pub fn map_ref<T: ?Sized>(
        self,
        f: impl Fn(&S::Item) -> &T + 'static,
    ) -> ObsRef<impl ObservableRef<Item = T>> {
        struct MapRef<S, F> {
            source: S,
            f: F,
        }
        impl<S, F, T> ObservableRef for MapRef<S, F>
        where
            S: ObservableRef,
            F: Fn(&S::Item) -> &T + 'static,
            T: ?Sized,
        {
            type Item = T;
            fn with<U>(
                &self,
                f: impl FnOnce(&Self::Item, &BindContext) -> U,
                cx: &BindContext,
            ) -> U {
                self.source.with(|value, cx| f((self.f)(value), cx), cx)
            }
        }
        ObsRef(MapRef { source: self.0, f })
    }

    pub fn map_borrow<B: ?Sized + 'static>(self) -> ObsRef<impl ObservableRef<Item = B>>
    where
        S::Item: Borrow<B>,
    {
        struct MapBorrow<S, B>
        where
            S: ObservableRef,
            S::Item: Borrow<B>,
            B: ?Sized + 'static,
        {
            source: S,
            _phantom: PhantomData<fn(&S::Item) -> &B>,
        };
        impl<S, B> ObservableRef for MapBorrow<S, B>
        where
            S: ObservableRef,
            S::Item: Borrow<B>,
            B: ?Sized + 'static,
        {
            type Item = B;

            fn with<U>(
                &self,
                f: impl FnOnce(&Self::Item, &BindContext) -> U,
                cx: &BindContext,
            ) -> U {
                self.source.with(|value, cx| f(value.borrow(), cx), cx)
            }

            fn into_dyn(self) -> DynObsRef<Self::Item>
            where
                Self: Sized,
            {
                self.source.into_dyn().map_borrow()
            }
        }
        ObsRef(MapBorrow {
            source: self.0,
            _phantom: PhantomData,
        })
    }

    pub fn flat_map<U: Observable>(
        self,
        f: impl Fn(&S::Item) -> U + 'static,
    ) -> Obs<impl Observable<Item = U::Item>> {
        self.map(f).flatten()
    }
    pub fn flatten(self) -> Obs<impl Observable<Item = <S::Item as Observable>::Item>>
    where
        S::Item: Observable,
    {
        obs(move |cx| self.with(|value, cx| value.get(cx), cx))
    }
    pub fn map_async_with<Fut>(
        self,
        f: impl Fn(&S::Item) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> ObsBorrow<impl ObservableBorrow<Item = Poll<Fut::Output>> + Clone>
    where
        Fut: Future + 'static,
    {
        ObsBorrow(Rc::new(MapAsync::new(self.map(f), sp)))
    }
    pub fn scan<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, &S::Item) -> St + 'static,
    ) -> ObsBorrow<impl ObservableBorrow<Item = St> + Clone> {
        ObsBorrow(Rc::new(Scan::new(
            initial_state,
            scan_op(
                move |st, cx| {
                    let f = &f;
                    self.with(move |x, _| f(st, x), cx)
                },
                |st| st,
                |st| st,
            ),
        )))
    }
    pub fn filter_scan<St: 'static>(
        self,
        initial_state: St,
        predicate: impl Fn(&St, &S::Item) -> bool + 'static,
        f: impl Fn(St, &S::Item) -> St + 'static,
    ) -> ObsBorrow<impl ObservableBorrow<Item = St> + Clone> {
        ObsBorrow(Rc::new(FilterScan::new(
            initial_state,
            filter_scan_op(
                move |state, cx| {
                    self.with(
                        |value, _ctx| {
                            let is_notify = predicate(&state, &value);
                            let state = if is_notify { f(state, value) } else { state };
                            FilterScanLoad { is_notify, state }
                        },
                        cx,
                    )
                },
                |state| state,
                |state| state,
            ),
        )))
    }

    pub fn cloned(self) -> Obs<impl Observable<Item = S::Item>>
    where
        S::Item: Clone,
    {
        self.map(|x| x.clone())
    }
    pub fn fold<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, &S::Item) -> St + 'static,
    ) -> Fold<St> {
        let mut f = f;
        Fold::new(FoldBy::new(
            initial_state,
            fold_by_op(
                move |st, cx| {
                    let f = &mut f;
                    self.with(move |x, _| f(st, x), cx)
                },
                |st| st,
                |st| st,
            ),
        ))
    }
    pub fn collect_to<E: for<'a> Extend<&'a S::Item> + 'static>(self, e: E) -> Fold<E> {
        self.fold(e, |mut e, x| {
            e.extend(once(x));
            e
        })
    }
    pub fn collect<E: for<'a> Extend<&'a S::Item> + Default + 'static>(self) -> Fold<E> {
        self.collect_to(Default::default())
    }
    pub fn collect_vec(self) -> Fold<Vec<S::Item>>
    where
        S::Item: Copy,
    {
        self.collect()
    }
    pub fn for_each(self, f: impl FnMut(&S::Item) + 'static) -> Subscription {
        self.fold(f, move |mut f, x| {
            f(x);
            f
        })
        .into()
    }
    pub fn for_each_async_with<Fut>(
        self,
        f: impl FnMut(&S::Item) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        let mut f = f;
        Fold::new(FoldBy::new(
            (),
            fold_by_op(
                move |_, cx| ((), self.with(|x, _ctx| sp.spawn_local(f(x)), cx)),
                |_| (),
                |_| (),
            ),
        ))
        .into()
    }

    pub fn hot(self) -> ObsRef<impl ObservableRef<Item = S::Item>> {
        ObsRef(Hot::new(self))
    }
}
impl<S: ObservableRef> ObservableRef for ObsRef<S> {
    type Item = S::Item;
    fn with<U>(&self, f: impl FnOnce(&Self::Item, &BindContext) -> U, cx: &BindContext) -> U {
        self.0.with(f, cx)
    }
    fn into_dyn(self) -> DynObsRef<Self::Item>
    where
        Self: Sized,
    {
        self.0.into_dyn()
    }
}