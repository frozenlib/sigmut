use super::*;

pub fn re_ref<S, T, F>(this: S, f: F) -> ReRefOps<impl ReactiveRef<Item = T>>
where
    S: 'static,
    T: 'static + ?Sized,
    F: Fn(&S, &BindContext, &mut dyn FnMut(&BindContext, &T)) + 'static,
{
    struct ReRefFn<S, T: ?Sized, F> {
        this: S,
        f: F,
        _phantom: PhantomData<fn(&Self) -> &T>,
    }
    impl<S, T, F> ReactiveRef for ReRefFn<S, T, F>
    where
        S: 'static,
        T: 'static + ?Sized,
        F: Fn(&S, &BindContext, &mut dyn FnMut(&BindContext, &T)) + 'static,
    {
        type Item = T;

        fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &T) -> U) -> U {
            let mut result = None;
            let mut f = Some(f);
            (self.f)(&self.this, ctx, &mut |ctx, value| {
                result = Some((f.take().unwrap())(ctx, value));
            });
            result.unwrap()
        }
    }
    ReRefOps(ReRefFn {
        this,
        f,
        _phantom: PhantomData,
    })
}

pub fn re_ref_constant<T: 'static>(value: T) -> ReRefOps<impl ReactiveRef<Item = T>> {
    struct ReRefConstant<T>(T);
    impl<T: 'static> ReactiveRef for ReRefConstant<T> {
        type Item = T;
        fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &Self::Item) -> U) -> U {
            f(ctx, &self.0)
        }
    }
    ReRefOps(ReRefConstant(value))
}
pub fn re_ref_static<T>(value: &'static T) -> ReRefOps<impl ReactiveRef<Item = T>> {
    struct ReRefStatic<T: 'static>(&'static T);
    impl<T: 'static> ReactiveRef for ReRefStatic<T> {
        type Item = T;
        fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &Self::Item) -> U) -> U {
            f(ctx, self.0)
        }
    }
    ReRefOps(ReRefStatic(value))
}

#[derive(Clone)]
pub struct ReRefOps<S>(pub(super) S);

impl<S: ReactiveRef> ReRefOps<S> {
    pub fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &S::Item) -> U) -> U {
        self.0.with(ctx, f)
    }
    pub fn head_tail(self, scope: &BindContextScope, f: impl FnOnce(&S::Item)) -> TailRefOps<S> {
        TailRefOps::new(self.0, scope, f)
    }
    pub fn re_ref(self) -> ReRef<S::Item> {
        self.0.into_re_ref()
    }
    pub fn map<T: 'static>(
        self,
        f: impl Fn(&S::Item) -> T + 'static,
    ) -> ReOps<impl Reactive<Item = T>> {
        re(move |ctx| self.with(ctx, |_, x| f(x)))
    }
    pub fn map_ref<T: ?Sized>(
        self,
        f: impl Fn(&S::Item) -> &T + 'static,
    ) -> ReRefOps<impl ReactiveRef<Item = T>> {
        struct MapRef<S, F> {
            source: S,
            f: F,
        }
        impl<S, F, T> ReactiveRef for MapRef<S, F>
        where
            S: ReactiveRef,
            F: Fn(&S::Item) -> &T + 'static,
            T: ?Sized,
        {
            type Item = T;
            fn with<U>(
                &self,
                ctx: &BindContext,
                f: impl FnOnce(&BindContext, &Self::Item) -> U,
            ) -> U {
                self.source.with(ctx, |ctx, value| f(ctx, (self.f)(value)))
            }
        }
        ReRefOps(MapRef { source: self.0, f })
    }

    pub fn map_borrow<B: ?Sized + 'static>(self) -> ReRefOps<impl ReactiveRef<Item = B>>
    where
        S::Item: Borrow<B>,
    {
        struct MapBorrow<S, B>
        where
            S: ReactiveRef,
            S::Item: Borrow<B>,
            B: ?Sized + 'static,
        {
            source: S,
            _phantom: PhantomData<fn(&S::Item) -> &B>,
        };
        impl<S, B> ReactiveRef for MapBorrow<S, B>
        where
            S: ReactiveRef,
            S::Item: Borrow<B>,
            B: ?Sized + 'static,
        {
            type Item = B;

            fn with<U>(
                &self,
                ctx: &BindContext,
                f: impl FnOnce(&BindContext, &Self::Item) -> U,
            ) -> U {
                self.source.with(ctx, |ctx, value| f(ctx, value.borrow()))
            }

            fn into_re_ref(self) -> ReRef<Self::Item>
            where
                Self: Sized,
            {
                self.source.into_re_ref().map_borrow()
            }
        }
        ReRefOps(MapBorrow {
            source: self.0,
            _phantom: PhantomData,
        })
    }

    pub fn flat_map<U: Reactive>(
        self,
        f: impl Fn(&S::Item) -> U + 'static,
    ) -> ReOps<impl Reactive<Item = U::Item>> {
        self.map(f).flatten()
    }
    pub fn flatten(self) -> ReOps<impl Reactive<Item = <S::Item as Reactive>::Item>>
    where
        S::Item: Reactive,
    {
        re(move |ctx| self.with(ctx, |ctx, value| value.get(ctx)))
    }
    pub fn map_async_with<Fut>(
        self,
        f: impl Fn(&S::Item) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> ReBorrowOps<impl ReactiveBorrow<Item = Poll<Fut::Output>> + Clone>
    where
        Fut: Future + 'static,
    {
        ReBorrowOps(Rc::new(MapAsync::new(self.map(f), sp)))
    }
    pub fn scan<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, &S::Item) -> St + 'static,
    ) -> ReBorrowOps<impl ReactiveBorrow<Item = St> + Clone> {
        ReBorrowOps(Rc::new(Scan::new(
            initial_state,
            move |st, ctx| {
                let f = &f;
                self.with(ctx, move |_, x| f(st, x))
            },
            |st| st,
            |st| st,
        )))
    }
    pub fn filter_scan<St: 'static>(
        self,
        initial_state: St,
        predicate: impl Fn(&St, &S::Item) -> bool + 'static,
        f: impl Fn(St, &S::Item) -> St + 'static,
    ) -> ReBorrowOps<impl ReactiveBorrow<Item = St> + Clone> {
        ReBorrowOps(Rc::new(FilterScan::new(
            initial_state,
            move |state, ctx| {
                self.with(ctx, |_ctx, value| {
                    let is_notify = predicate(&state, &value);
                    let state = if is_notify { f(state, value) } else { state };
                    FilterScanResult { is_notify, state }
                })
            },
            |state| state,
            |state| state,
        )))
    }

    pub fn cloned(self) -> ReOps<impl Reactive<Item = S::Item>>
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
            move |st, ctx| {
                let f = &mut f;
                (self.with(ctx, move |_ctx, x| f(st, x)), ())
            },
            |(st, _)| st,
            |st| st,
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
            move |_, ctx| ((), self.with(ctx, |_ctx, x| sp.spawn_local(f(x)))),
            |_| (),
            |_| (),
        ))
        .into()
    }

    pub fn hot(self) -> ReRefOps<impl ReactiveRef<Item = S::Item>> {
        ReRefOps(Hot::new(self))
    }
}
impl<S: ReactiveRef> ReactiveRef for ReRefOps<S> {
    type Item = S::Item;
    fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &Self::Item) -> U) -> U {
        self.0.with(ctx, f)
    }
    fn into_re_ref(self) -> ReRef<Self::Item>
    where
        Self: Sized,
    {
        self.0.into_re_ref()
    }
}
