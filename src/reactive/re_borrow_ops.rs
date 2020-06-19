use super::*;
use std::cell::Ref;

pub fn re_borrow<S, T>(
    this: S,
    borrow: impl for<'a> Fn(&'a S, &BindContext<'a>) -> Ref<'a, T> + 'static,
) -> ReBorrowOps<impl ReactiveBorrow<Item = T>>
where
    T: 'static + ?Sized,
    S: 'static,
{
    struct ReBorrowFn<S, F> {
        this: S,
        borrow: F,
    }
    impl<T, S, F> ReactiveBorrow for ReBorrowFn<S, F>
    where
        T: 'static + ?Sized,
        S: 'static,
        for<'a> F: Fn(&'a S, &BindContext<'a>) -> Ref<'a, T> + 'static,
    {
        type Item = T;
        fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, T> {
            (self.borrow)(&self.this, ctx)
        }
    }

    ReBorrowOps(ReBorrowFn { this, borrow })
}
pub fn re_borrow_constant<T: 'static>(value: T) -> ReBorrowOps<impl ReactiveBorrow<Item = T>> {
    re_borrow(RefCell::new(value), |this, _| this.borrow())
}

#[derive(Clone)]
pub struct ReBorrowOps<S>(pub(super) S);

impl<S: ReactiveBorrow> ReBorrowOps<S> {
    pub fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, S::Item> {
        self.0.borrow(ctx)
    }
    pub fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &S::Item) -> U) -> U {
        f(ctx, &self.borrow(ctx))
    }
    pub fn head_tail<'a>(
        &'a self,
        scope: &'a BindContextScope,
    ) -> (
        Ref<'a, S::Item>,
        TailRefOps<impl ReactiveRef<Item = S::Item>>,
    )
    where
        S: Clone,
    {
        TailRefOps::new_borrow(self, scope, |s| s.clone().ops_ref())
    }

    pub fn ops_ref(self) -> ReRefOps<ReRefByReBorrow<S>> {
        ReRefOps(ReRefByReBorrow(self))
    }
    pub fn ops_any(self) -> ReBorrowOps<ReBorrow<S::Item>> {
        ReBorrowOps(self.into_dyn())
    }
    pub fn into_dyn(self) -> ReBorrow<S::Item> {
        self.0.into_dyn()
    }
    pub fn into_dyn_ref(self) -> ReRef<S::Item> {
        self.into_dyn().to_re_ref()
    }
    pub fn map<T>(self, f: impl Fn(&S::Item) -> T + 'static) -> ReOps<impl Reactive<Item = T>> {
        re(move |ctx| f(&self.borrow(ctx)))
    }
    pub fn map_ref<T: ?Sized + 'static>(
        self,
        f: impl Fn(&S::Item) -> &T + 'static,
    ) -> ReBorrowOps<impl ReactiveBorrow<Item = T>> {
        re_borrow(self, move |this, ctx| Ref::map(this.borrow(ctx), &f))
    }
    pub fn map_borrow<B: ?Sized + 'static>(self) -> ReBorrowOps<impl ReactiveBorrow<Item = B>>
    where
        S::Item: Borrow<B>,
    {
        struct MapBorrow<S, B>
        where
            S: ReactiveBorrow,
            S::Item: Borrow<B>,
            B: ?Sized + 'static,
        {
            source: S,
            _phantom: PhantomData<fn(&S::Item) -> &B>,
        };
        impl<S, B> ReactiveBorrow for MapBorrow<S, B>
        where
            S: ReactiveBorrow,
            S::Item: Borrow<B>,
            B: ?Sized + 'static,
        {
            type Item = B;

            fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item> {
                Ref::map(self.source.borrow(ctx), |x| x.borrow())
            }
            fn into_dyn(self) -> ReBorrow<Self::Item>
            where
                Self: Sized,
            {
                self.source.into_dyn().map_borrow()
            }
        }
        ReBorrowOps(MapBorrow {
            source: self,
            _phantom: PhantomData,
        })
    }
    pub fn flat_map<U: Reactive>(
        self,
        f: impl Fn(&S::Item) -> U + 'static,
    ) -> ReOps<impl Reactive<Item = U::Item>> {
        re(move |ctx| f(&self.borrow(ctx)).get(ctx))
    }

    pub fn flatten(self) -> ReOps<impl Reactive<Item = <S::Item as Reactive>::Item>>
    where
        S::Item: Reactive,
    {
        re(move |ctx| self.borrow(ctx).get(ctx))
    }

    pub fn map_async_with<Fut>(
        self,
        f: impl Fn(&S::Item) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> ReBorrowOps<impl ReactiveBorrow<Item = Poll<Fut::Output>> + Clone>
    where
        Fut: Future + 'static,
    {
        self.ops_ref().map_async_with(f, sp)
    }
    pub fn cloned(self) -> ReOps<impl Reactive<Item = S::Item>>
    where
        S::Item: Clone,
    {
        self.map(|x| x.clone())
    }
    pub fn scan<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, &S::Item) -> St + 'static,
    ) -> ReBorrowOps<impl ReactiveBorrow<Item = St> + Clone> {
        self.ops_ref().scan(initial_state, f)
    }
    pub fn filter_scan<St: 'static>(
        self,
        initial_state: St,
        predicate: impl Fn(&St, &S::Item) -> bool + 'static,
        f: impl Fn(St, &S::Item) -> St + 'static,
    ) -> ReBorrowOps<impl ReactiveBorrow<Item = St> + Clone> {
        self.ops_ref().filter_scan(initial_state, predicate, f)
    }

    pub fn fold<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, &S::Item) -> St + 'static,
    ) -> Fold<St> {
        self.ops_ref().fold(initial_state, f)
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
    pub fn to_vec(self) -> Fold<Vec<S::Item>>
    where
        S::Item: Copy,
    {
        self.collect()
    }

    pub fn for_each(self, f: impl FnMut(&S::Item) + 'static) -> Subscription {
        self.ops_ref().for_each(f)
    }
    pub fn for_each_async_with<Fut>(
        self,
        f: impl FnMut(&S::Item) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        self.ops_ref().for_each_async_with(f, sp)
    }
    pub fn hot(self) -> ReBorrowOps<impl ReactiveBorrow<Item = S::Item>> {
        ReBorrowOps(Hot::new(self))
    }
}

impl<S: ReactiveBorrow> ReactiveBorrow for ReBorrowOps<S> {
    type Item = S::Item;
    fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.0.borrow(ctx)
    }
    fn into_dyn(self) -> ReBorrow<Self::Item>
    where
        Self: Sized,
    {
        self.0.into_dyn()
    }
}

#[derive(Clone)]
pub struct ReRefByReBorrow<S>(ReBorrowOps<S>);
impl<S: ReactiveBorrow> ReactiveRef for ReRefByReBorrow<S> {
    type Item = S::Item;
    fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &Self::Item) -> U) -> U {
        self.0.with(ctx, f)
    }
    fn into_dyn(self) -> ReRef<Self::Item>
    where
        Self: Sized,
    {
        self.0.into_dyn_ref()
    }
}
