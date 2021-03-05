use std::{borrow::Borrow, iter::once};

use crate::*;

#[derive(Clone)]
pub struct Obs<S>(pub(super) S);

impl<S: Observable> Obs<S> {
    pub fn get(&self, cx: &mut BindContext) -> <S::Item as ToOwned>::Owned
    where
        S::Item: ToOwned,
    {
        self.with(|value, _| value.to_owned(), cx)
    }
    pub fn get_head(&self) -> <S::Item as ToOwned>::Owned
    where
        S::Item: ToOwned,
    {
        BindContext::nul(|cx| self.get(cx))
    }

    pub fn with<U>(
        &self,
        f: impl FnOnce(&S::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        self.0.with(f, cx)
    }
    pub fn with_head<U>(&self, f: impl FnOnce(&S::Item) -> U) -> U {
        BindContext::nul(|cx| self.with(|value, _| f(value), cx))
    }
    // pub fn head_tail<U>(self, f: impl FnOnce(&S::Item) -> U) -> (U, TailRef<S>) {
    //     BindScope::with(|scope| self.head_tail_with(scope, f))
    // }
    // pub fn head_tail_with<U>(
    //     self,
    //     scope: &BindScope,
    //     f: impl FnOnce(&S::Item) -> U,
    // ) -> (U, TailRef<S>) {
    //     TailRef::new(self.0, scope, f)
    // }
    // pub fn into_dyn(self) -> DynObsRef<S::Item> {
    //     self.0.into_dyn_obs_ref()
    // }

    #[inline]
    pub fn map<T: 'static>(
        self,
        f: impl Fn(&S::Item) -> T + 'static,
    ) -> Obs<impl Observable<Item = T>> {
        obs(move |cx| self.with(|x, _| f(x), cx))
    }

    #[inline]
    pub fn map_ref<T: ?Sized + 'static>(
        self,
        f: impl Fn(&S::Item) -> &T + 'static,
    ) -> Obs<impl Observable<Item = T>> {
        struct MapRef<S, F> {
            s: S,
            f: F,
        }
        impl<S, F, T> Observable for MapRef<S, F>
        where
            S: Observable,
            F: Fn(&S::Item) -> &T + 'static,
            T: ?Sized + 'static,
        {
            type Item = T;

            #[inline]
            fn with<U>(
                &self,
                f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
                cx: &mut BindContext,
            ) -> U {
                self.s.with(|value, cx| f((self.f)(value), cx), cx)
            }
        }

        Obs(MapRef { s: self, f })
    }

    // pub fn map_borrow<B: ?Sized + 'static>(self) -> ObsRef<impl ObservableRef<Item = B>>
    // where
    //     S::Item: Borrow<B>,
    // {
    //     struct MapBorrow<S, B>
    //     where
    //         S: ObservableRef,
    //         S::Item: Borrow<B>,
    //         B: ?Sized + 'static,
    //     {
    //         source: S,
    //         _phantom: PhantomData<fn(&S::Item) -> &B>,
    //     }
    //     impl<S, B> ObservableRef for MapBorrow<S, B>
    //     where
    //         S: ObservableRef,
    //         S::Item: Borrow<B>,
    //         B: ?Sized + 'static,
    //     {
    //         type Item = B;

    //         fn with<U>(
    //             &self,
    //             f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
    //             cx: &mut BindContext,
    //         ) -> U {
    //             self.source.with(|value, cx| f(value.borrow(), cx), cx)
    //         }

    //         fn into_dyn_obs_ref(self) -> DynObsRef<Self::Item>
    //         where
    //             Self: Sized,
    //         {
    //             self.source.into_dyn_obs_ref().map_borrow()
    //         }
    //     }
    //     ObsRef(MapBorrow {
    //         source: self.0,
    //         _phantom: PhantomData,
    //     })
    // }

    // pub fn map_as_ref<U: ?Sized + 'static>(self) -> ObsRef<impl ObservableRef<Item = U>>
    // where
    //     S::Item: AsRef<U>,
    // {
    //     struct MapAsRef<S, T>
    //     where
    //         S: ObservableRef,
    //         S::Item: AsRef<T>,
    //         T: ?Sized + 'static,
    //     {
    //         source: S,
    //         _phantom: PhantomData<fn(&S::Item) -> &T>,
    //     }
    //     impl<S, T> ObservableRef for MapAsRef<S, T>
    //     where
    //         S: ObservableRef,
    //         S::Item: AsRef<T>,
    //         T: ?Sized + 'static,
    //     {
    //         type Item = T;

    //         fn with<U>(
    //             &self,
    //             f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
    //             cx: &mut BindContext,
    //         ) -> U {
    //             self.source.with(|value, cx| f(value.as_ref(), cx), cx)
    //         }

    //         fn into_dyn_obs_ref(self) -> DynObsRef<Self::Item>
    //         where
    //             Self: Sized,
    //         {
    //             self.source.into_dyn_obs_ref().map_as_ref()
    //         }
    //     }
    //     ObsRef(MapAsRef {
    //         source: self.0,
    //         _phantom: PhantomData,
    //     })
    // }

    #[inline]
    pub fn flat_map<U: Observable>(
        self,
        f: impl Fn(&S::Item) -> U + 'static,
    ) -> Obs<impl Observable<Item = U::Item>> {
        self.map(f).flatten()
    }

    #[inline]
    pub fn flat_map_ref<U: Observable>(
        self,
        f: impl Fn(&S::Item) -> &U + 'static,
    ) -> Obs<impl Observable<Item = U::Item>> {
        self.map_ref(f).flatten()
    }

    #[inline]
    pub fn flatten(self) -> Obs<impl Observable<Item = <S::Item as Observable>::Item>>
    where
        S::Item: Observable,
    {
        struct Flatten<S>(S);
        impl<S> Observable for Flatten<S>
        where
            S: Observable,
            S::Item: Observable,
        {
            type Item = <S::Item as Observable>::Item;

            #[inline]
            fn with<U>(
                &self,
                f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
                cx: &mut BindContext,
            ) -> U {
                self.0
                    .with(|s, cx| s.with(|value, cx| f(value, cx), cx), cx)
            }
        }
        Obs(Flatten(self))
    }

    // pub fn map_async_with<Fut>(
    //     self,
    //     f: impl Fn(&S::Item) -> Fut + 'static,
    //     sp: impl LocalSpawn,
    // ) -> ObsRef<impl ObservableRef<Item = Poll<Fut::Output>> + Clone>
    // where
    //     Fut: Future + 'static,
    // {
    //     ObsRef(Rc::new(MapAsync::new(self.map(f), sp)))
    // }
    pub fn scan<St: 'static>(
        self,
        initial_state: St,
        mut f: impl FnMut(&mut St, &S::Item) + 'static,
    ) -> Obs<impl Observable<Item = St>> {
        obs_scan(initial_state, move |st, cx| {
            let f = &mut f;
            self.with(|value, _cx| f(st, value), cx)
        })
    }
    pub fn filter_scan<St: 'static>(
        self,
        initial_state: St,
        predicate: impl Fn(&St, &S::Item) -> bool + 'static,
        f: impl FnMut(&mut St, &S::Item) + 'static,
    ) -> Obs<impl Observable<Item = St>> {
        self.filter_scan_map(initial_state, predicate, f, |x| x)
    }
    pub fn filter_scan_map<St, T>(
        self,
        initial_state: St,
        predicate: impl Fn(&St, &S::Item) -> bool + 'static,
        mut f: impl FnMut(&mut St, &S::Item) + 'static,
        m: impl Fn(&St) -> &T + 'static,
    ) -> Obs<impl Observable<Item = T>>
    where
        St: 'static,
        T: ?Sized + 'static,
    {
        obs_filter_scan_map(
            initial_state,
            move |st, cx| {
                let f = &mut f;
                self.with(
                    |value, _cx| {
                        if predicate(st, value) {
                            f(st, value);
                            true
                        } else {
                            false
                        }
                    },
                    cx,
                )
            },
            m,
        )
    }

    pub fn dedup_by(
        self,
        eq: impl Fn(&S::Item, &S::Item) -> bool + 'static,
    ) -> Obs<impl Observable<Item = S::Item>>
    where
        S::Item: ToOwned,
    {
        let initial_state: Option<<S::Item as ToOwned>::Owned> = None;
        self.filter_scan_map(
            initial_state,
            move |st, value| {
                if let Some(old) = st {
                    eq(old.borrow(), value)
                } else {
                    true
                }
            },
            |st, value| *st = Some(value.to_owned()),
            |st| st.as_ref().unwrap().borrow(),
        )
    }
    pub fn dedup_by_key<K>(
        self,
        to_key: impl Fn(&S::Item) -> K + 'static,
    ) -> Obs<impl Observable<Item = S::Item>>
    where
        K: PartialEq,
        S::Item: ToOwned,
    {
        self.dedup_by(move |old, new| to_key(old) == to_key(new))
    }
    pub fn dedup(self) -> Obs<impl Observable<Item = S::Item>>
    where
        S::Item: ToOwned + PartialEq,
    {
        self.dedup_by(move |old, new| old == new)
    }

    pub fn fold<St: 'static>(
        self,
        st: St,
        mut f: impl FnMut(&mut St, &S::Item) + 'static,
    ) -> Fold<St> {
        Fold::new(st, move |st, cx| self.with(|value, _cx| f(st, value), cx))
    }
    pub fn collect_to<E>(self, e: E) -> Fold<E>
    where
        S::Item: ToOwned,
        E: Extend<<S::Item as ToOwned>::Owned> + 'static,
    {
        self.fold(e, |e, x| e.extend(once(x.to_owned())))
    }
    pub fn collect<E>(self) -> Fold<E>
    where
        S::Item: ToOwned,
        E: Extend<<S::Item as ToOwned>::Owned> + Default + 'static,
    {
        self.collect_to(Default::default())
    }
    pub fn collect_vec(self) -> Fold<Vec<<S::Item as ToOwned>::Owned>>
    where
        S::Item: ToOwned,
    {
        self.collect()
    }
    pub fn subscribe(self, mut f: impl FnMut(&S::Item) + 'static) -> Subscription {
        subscribe(move |cx| self.with(|value, _cx| f(value), cx))
    }
    pub fn subscribe_to<O>(self, o: O) -> impl Subscriber<O>
    where
        for<'a> O: Observer<&'a S::Item>,
    {
        subscribe_to(o, move |o, cx| self.with(|value, _cx| o.next(value), cx))
    }
    // pub fn subscribe_async_with<Fut>(
    //     self,
    //     f: impl FnMut(&S::Item) -> Fut + 'static,
    //     sp: impl LocalSpawn,
    // ) -> Subscription
    // where
    //     Fut: Future<Output = ()> + 'static,
    // {
    //     let mut f = f;
    //     Fold::new(FoldBy::new(
    //         (),
    //         fold_by_op(
    //             move |_, cx| ((), self.with(|x, _ctx| sp.spawn_local(f(x)), cx)),
    //             |_| (),
    //             |_| (),
    //         ),
    //     ))
    //     .into()
    // }

    // pub fn hot(self) -> ObsRef<impl ObservableRef<Item = S::Item>> {
    //     ObsRef(Hot::new(self))
    // }
}
impl<S: Observable> Observable for Obs<S> {
    type Item = S::Item;
    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        Obs::with(self, f, cx)
    }

    fn into_dyn(self) -> DynObs<Self::Item> {
        self.0.into_dyn()
    }
}
