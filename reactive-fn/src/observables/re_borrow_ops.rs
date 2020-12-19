use super::*;
use std::cell::Ref;

pub fn re_borrow<S, T>(
    this: S,
    borrow: impl for<'a> Fn(&'a S, &BindContext<'a>) -> Ref<'a, T> + 'static,
) -> ReBorrowOps<impl ObservableBorrow<Item = T>>
where
    T: 'static + ?Sized,
    S: 'static,
{
    struct ReBorrowFn<S, F> {
        this: S,
        borrow: F,
    }
    impl<T, S, F> ObservableBorrow for ReBorrowFn<S, F>
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

    ReBorrowOps(ReBorrowFn { this, borrow })
}
pub fn re_borrow_constant<T: 'static>(value: T) -> ReBorrowOps<impl ObservableBorrow<Item = T>> {
    re_borrow(RefCell::new(value), |this, _| this.borrow())
}

#[derive(Clone)]
pub struct ReBorrowOps<S>(pub(super) S);

impl<S: ObservableBorrow> ReBorrowOps<S> {
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

    pub fn as_ref(self) -> ReRefOps<ReRefByReBorrow<S>> {
        ReRefOps(ReRefByReBorrow(self))
    }
    pub fn as_any(self) -> ReBorrowOps<DynObsBorrow<S::Item>> {
        ReBorrowOps(self.re_borrow())
    }
    pub fn re_borrow(self) -> DynObsBorrow<S::Item> {
        self.0.into_re_borrow()
    }
    pub fn re_ref(self) -> ReRef<S::Item> {
        self.re_borrow().as_ref()
    }
    pub fn map<T>(self, f: impl Fn(&S::Item) -> T + 'static) -> ReOps<impl Observable<Item = T>> {
        re(move |cx| f(&self.borrow(cx)))
    }
    pub fn map_ref<T: ?Sized + 'static>(
        self,
        f: impl Fn(&S::Item) -> &T + 'static,
    ) -> ReBorrowOps<impl ObservableBorrow<Item = T>> {
        re_borrow(self, move |this, cx| Ref::map(this.borrow(cx), &f))
    }
    pub fn map_borrow<B: ?Sized + 'static>(self) -> ReBorrowOps<impl ObservableBorrow<Item = B>>
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
            fn into_re_borrow(self) -> DynObsBorrow<Self::Item>
            where
                Self: Sized,
            {
                self.source.into_re_borrow().map_borrow()
            }
        }
        ReBorrowOps(MapBorrow {
            source: self,
            _phantom: PhantomData,
        })
    }
    pub fn flat_map<U: Observable>(
        self,
        f: impl Fn(&S::Item) -> U + 'static,
    ) -> ReOps<impl Observable<Item = U::Item>> {
        re(move |cx| f(&self.borrow(cx)).get(cx))
    }

    pub fn flatten(self) -> ReOps<impl Observable<Item = <S::Item as Observable>::Item>>
    where
        S::Item: Observable,
    {
        re(move |cx| self.borrow(cx).get(cx))
    }

    pub fn map_async_with<Fut>(
        self,
        f: impl Fn(&S::Item) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> ReBorrowOps<impl ObservableBorrow<Item = Poll<Fut::Output>> + Clone>
    where
        Fut: Future + 'static,
    {
        self.as_ref().map_async_with(f, sp)
    }
    pub fn cloned(self) -> ReOps<impl Observable<Item = S::Item>>
    where
        S::Item: Clone,
    {
        self.map(|x| x.clone())
    }
    pub fn scan<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, &S::Item) -> St + 'static,
    ) -> ReBorrowOps<impl ObservableBorrow<Item = St> + Clone> {
        self.as_ref().scan(initial_state, f)
    }
    pub fn filter_scan<St: 'static>(
        self,
        initial_state: St,
        predicate: impl Fn(&St, &S::Item) -> bool + 'static,
        f: impl Fn(St, &S::Item) -> St + 'static,
    ) -> ReBorrowOps<impl ObservableBorrow<Item = St> + Clone> {
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
    pub fn hot(self) -> ReBorrowOps<impl ObservableBorrow<Item = S::Item>> {
        ReBorrowOps(Hot::new(self))
    }
}

impl<S: ObservableBorrow> ObservableBorrow for ReBorrowOps<S> {
    type Item = S::Item;
    fn borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.0.borrow(cx)
    }
    fn into_re_borrow(self) -> DynObsBorrow<Self::Item>
    where
        Self: Sized,
    {
        self.0.into_re_borrow()
    }
}

#[derive(Clone)]
pub struct ReRefByReBorrow<S>(ReBorrowOps<S>);
impl<S: ObservableBorrow> ObservableRef for ReRefByReBorrow<S> {
    type Item = S::Item;
    fn with<U>(&self, f: impl FnOnce(&Self::Item, &BindContext) -> U, cx: &BindContext) -> U {
        self.0.with(f, cx)
    }
    fn into_re_ref(self) -> ReRef<Self::Item>
    where
        Self: Sized,
    {
        self.0.re_ref()
    }
}
