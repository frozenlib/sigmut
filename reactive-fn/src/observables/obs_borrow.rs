use super::*;
use std::cell::Ref;

pub fn obs_borrow<S, T>(
    this: S,
    borrow: impl for<'a> Fn(&'a S, &BindContext<'a>) -> Ref<'a, T> + 'static,
) -> ObsBorrow<impl ObservableBorrow<Item = T>>
where
    T: 'static + ?Sized,
    S: 'static,
{
    struct ObsBorrowFn<S, F> {
        this: S,
        borrow: F,
    }
    impl<T, S, F> ObservableBorrow for ObsBorrowFn<S, F>
    where
        T: 'static + ?Sized,
        S: 'static,
        for<'a> F: Fn(&'a S, &BindContext<'a>) -> Ref<'a, T> + 'static,
    {
        type Item = T;
        fn borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, T> {
            (self.borrow)(&self.this, cx)
        }
    }

    ObsBorrow(ObsBorrowFn { this, borrow })
}
pub fn obs_borrow_constant<T: 'static>(value: T) -> ObsBorrow<impl ObservableBorrow<Item = T>> {
    obs_borrow(RefCell::new(value), |this, _| this.borrow())
}

#[derive(Clone)]
pub struct ObsBorrow<S>(pub(super) S);

impl<S: ObservableBorrow> ObsBorrow<S> {
    pub fn borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, S::Item> {
        self.0.borrow(cx)
    }
    pub fn with<U>(&self, f: impl FnOnce(&S::Item, &BindContext) -> U, cx: &BindContext) -> U {
        f(&self.borrow(cx), cx)
    }
    pub fn head_tail_with<'a>(
        &'a self,
        scope: &'a BindScope,
    ) -> (
        Ref<'a, S::Item>,
        TailRefOps<impl ObservableRef<Item = S::Item>>,
    )
    where
        S: Clone,
    {
        TailRefOps::new_borrow(self, scope, |s| s.clone().as_ref())
    }

    pub fn as_ref(self) -> ObsRef<ObsRefByObsBorrow<S>> {
        ObsRef(ObsRefByObsBorrow(self))
    }
    pub fn as_any(self) -> ObsBorrow<DynObsBorrow<S::Item>> {
        ObsBorrow(self.into_dyn())
    }
    pub fn into_dyn(self) -> DynObsBorrow<S::Item> {
        self.0.into_dyn()
    }
    pub fn into_dyn_ref(self) -> DynObsRef<S::Item> {
        self.into_dyn().as_ref()
    }
    pub fn map<T>(self, f: impl Fn(&S::Item) -> T + 'static) -> Obs<impl Observable<Item = T>> {
        obs(move |cx| f(&self.borrow(cx)))
    }
    pub fn map_ref<T: ?Sized + 'static>(
        self,
        f: impl Fn(&S::Item) -> &T + 'static,
    ) -> ObsBorrow<impl ObservableBorrow<Item = T>> {
        obs_borrow(self, move |this, cx| Ref::map(this.borrow(cx), &f))
    }
    pub fn map_borrow<B: ?Sized + 'static>(self) -> ObsBorrow<impl ObservableBorrow<Item = B>>
    where
        S::Item: Borrow<B>,
    {
        struct MapBorrow<S, B>
        where
            S: ObservableBorrow,
            S::Item: Borrow<B>,
            B: ?Sized + 'static,
        {
            source: S,
            _phantom: PhantomData<fn(&S::Item) -> &B>,
        };
        impl<S, B> ObservableBorrow for MapBorrow<S, B>
        where
            S: ObservableBorrow,
            S::Item: Borrow<B>,
            B: ?Sized + 'static,
        {
            type Item = B;

            fn borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item> {
                Ref::map(self.source.borrow(cx), |x| x.borrow())
            }
            fn into_dyn(self) -> DynObsBorrow<Self::Item>
            where
                Self: Sized,
            {
                self.source.into_dyn().map_borrow()
            }
        }
        ObsBorrow(MapBorrow {
            source: self,
            _phantom: PhantomData,
        })
    }
    pub fn flat_map<U: Observable>(
        self,
        f: impl Fn(&S::Item) -> U + 'static,
    ) -> Obs<impl Observable<Item = U::Item>> {
        obs(move |cx| f(&self.borrow(cx)).get(cx))
    }

    pub fn flatten(self) -> Obs<impl Observable<Item = <S::Item as Observable>::Item>>
    where
        S::Item: Observable,
    {
        obs(move |cx| self.borrow(cx).get(cx))
    }

    pub fn map_async_with<Fut>(
        self,
        f: impl Fn(&S::Item) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> ObsBorrow<impl ObservableBorrow<Item = Poll<Fut::Output>> + Clone>
    where
        Fut: Future + 'static,
    {
        self.as_ref().map_async_with(f, sp)
    }
    pub fn cloned(self) -> Obs<impl Observable<Item = S::Item>>
    where
        S::Item: Clone,
    {
        self.map(|x| x.clone())
    }
    pub fn scan<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, &S::Item) -> St + 'static,
    ) -> ObsBorrow<impl ObservableBorrow<Item = St> + Clone> {
        self.as_ref().scan(initial_state, f)
    }
    pub fn filter_scan<St: 'static>(
        self,
        initial_state: St,
        predicate: impl Fn(&St, &S::Item) -> bool + 'static,
        f: impl Fn(St, &S::Item) -> St + 'static,
    ) -> ObsBorrow<impl ObservableBorrow<Item = St> + Clone> {
        self.as_ref().filter_scan(initial_state, predicate, f)
    }

    pub fn fold<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, &S::Item) -> St + 'static,
    ) -> Fold<St> {
        self.as_ref().fold(initial_state, f)
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
        self.as_ref().for_each(f)
    }
    pub fn for_each_async_with<Fut>(
        self,
        f: impl FnMut(&S::Item) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        self.as_ref().for_each_async_with(f, sp)
    }
    pub fn hot(self) -> ObsBorrow<impl ObservableBorrow<Item = S::Item>> {
        ObsBorrow(Hot::new(self))
    }
}

impl<S: ObservableBorrow> ObservableBorrow for ObsBorrow<S> {
    type Item = S::Item;
    fn borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.0.borrow(cx)
    }
    fn into_dyn(self) -> DynObsBorrow<Self::Item>
    where
        Self: Sized,
    {
        self.0.into_dyn()
    }
}

#[derive(Clone)]
pub struct ObsRefByObsBorrow<S>(ObsBorrow<S>);
impl<S: ObservableBorrow> ObservableRef for ObsRefByObsBorrow<S> {
    type Item = S::Item;
    fn with<U>(&self, f: impl FnOnce(&Self::Item, &BindContext) -> U, cx: &BindContext) -> U {
        self.0.with(f, cx)
    }
    fn into_dyn(self) -> DynObsRef<Self::Item>
    where
        Self: Sized,
    {
        self.0.into_dyn_ref()
    }
}
