use super::*;
use std::cell::Ref;

pub trait Reactive: 'static {
    type Item;
    fn get(&self, ctx: &BindContext) -> Self::Item;

    fn into_dyn(self) -> Re<Self::Item>
    where
        Self: Sized,
    {
        struct IntoDyn<S>(S);
        impl<S: Reactive> DynRe for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_get(&self, ctx: &BindContext) -> Self::Item {
                self.0.get(ctx)
            }

            fn to_re_ref(self: Rc<Self>) -> ReRef<Self::Item> {
                ReRef(ReRefData::Dyn(self as Rc<dyn DynReRef<Item = Self::Item>>))
            }
        }
        impl<S: Reactive> DynReRef for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_with(&self, ctx: &BindContext, f: &mut dyn FnMut(&BindContext, &Self::Item)) {
                f(ctx, &self.0.get(ctx))
            }
        }
        Re::from_dyn(IntoDyn(self))
    }
}

pub trait ReactiveBorrow: 'static {
    type Item: ?Sized;
    fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item>;

    fn into_dyn(self) -> ReBorrow<Self::Item>
    where
        Self: Sized,
    {
        struct IntoDyn<S>(S);
        impl<S: ReactiveBorrow> DynReBorrow for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item> {
                self.0.borrow(ctx)
            }
        }
        impl<S: ReactiveBorrow> DynReRef for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_with(&self, ctx: &BindContext, f: &mut dyn FnMut(&BindContext, &Self::Item)) {
                f(ctx, &self.0.borrow(ctx))
            }
        }
        ReBorrow::from_dyn(IntoDyn(self))
    }
}

pub trait ReactiveRef: 'static {
    type Item: ?Sized;
    fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &Self::Item) -> U) -> U;

    fn into_dyn(self) -> ReRef<Self::Item>
    where
        Self: Sized,
    {
        struct IntoDyn<S>(S);
        impl<S: ReactiveRef> DynReRef for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_with(&self, ctx: &BindContext, f: &mut dyn FnMut(&BindContext, &Self::Item)) {
                self.0.with(ctx, f)
            }
        }
        ReRef::from_dyn(IntoDyn(self))
    }
}

pub fn re<T>(get: impl Fn(&BindContext) -> T + 'static) -> ReOps<impl Reactive<Item = T>> {
    struct ReFn<F>(F);
    impl<F: Fn(&BindContext) -> T + 'static, T> Reactive for ReFn<F> {
        type Item = T;
        fn get(&self, ctx: &BindContext) -> Self::Item {
            (self.0)(ctx)
        }
    }

    ReOps(ReFn(get))
}
pub fn re_constant<T: 'static + Clone>(value: T) -> ReOps<impl Reactive<Item = T>> {
    re(move |_| value.clone())
}

#[derive(Clone)]
pub struct ReOps<S>(pub S);

impl<S: Reactive> ReOps<S> {
    pub fn get(&self, ctx: &BindContext) -> S::Item {
        self.0.get(ctx)
    }
    pub fn with<T>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &S::Item) -> T) -> T {
        f(ctx, &self.get(ctx))
    }
    pub fn map<T>(self, f: impl Fn(S::Item) -> T + 'static) -> ReOps<impl Reactive<Item = T>> {
        re(move |ctx| f(self.get(ctx)))
    }
    pub fn flat_map<T: Reactive>(
        self,
        f: impl Fn(S::Item) -> T + 'static,
    ) -> ReOps<impl Reactive<Item = T::Item>> {
        re(move |ctx| f(self.get(ctx)).get(ctx))
    }
    pub fn flatten(self) -> ReOps<impl Reactive<Item = <S::Item as Reactive>::Item>>
    where
        S::Item: Reactive,
    {
        re(move |ctx| self.get(ctx).get(ctx))
    }
    pub fn map_async_with<Fut>(
        self,
        f: impl Fn(S::Item) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> ReBorrowOps<impl ReactiveBorrow<Item = Poll<Fut::Output>> + Clone>
    where
        Fut: Future + 'static,
    {
        ReBorrowOps(Rc::new(MapAsync::new(self.map(f), sp)))
    }

    pub fn ops_ref(self) -> ReRefOps<impl ReactiveRef<Item = S::Item>> {
        struct ReRefByRe<S>(ReOps<S>);
        impl<S: Reactive> ReactiveRef for ReRefByRe<S> {
            type Item = S::Item;
            fn with<U>(
                &self,
                ctx: &BindContext,
                f: impl FnOnce(&BindContext, &Self::Item) -> U,
            ) -> U {
                self.0.with(ctx, f)
            }
            fn into_dyn(self) -> ReRef<Self::Item>
            where
                Self: Sized,
            {
                self.0.into_dyn_ref()
            }
        }
        ReRefOps(ReRefByRe(self))
    }
    pub fn ops_any(self) -> ReOps<Re<S::Item>> {
        ReOps(self.into_dyn())
    }
    pub fn into_dyn(self) -> Re<S::Item> {
        self.0.into_dyn()
    }
    pub fn into_dyn_ref(self) -> ReRef<S::Item> {
        self.0.into_dyn().to_re_ref()
    }

    pub fn cached(self) -> ReBorrowOps<impl ReactiveBorrow<Item = S::Item> + Clone> {
        ReBorrowOps(Rc::new(Scan::new(
            (),
            move |_, ctx| self.get(ctx),
            |_| (),
            |x| x,
        )))
    }
    pub fn scan<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, S::Item) -> St + 'static,
    ) -> ReBorrowOps<impl ReactiveBorrow<Item = St> + Clone> {
        ReBorrowOps(Rc::new(Scan::new(
            initial_state,
            move |st, ctx| f(st, self.get(ctx)),
            |st| st,
            |st| st,
        )))
    }
    pub fn filter_scan<St: 'static>(
        self,
        initial_state: St,
        predicate: impl Fn(&St, &S::Item) -> bool + 'static,
        f: impl Fn(St, S::Item) -> St + 'static,
    ) -> ReBorrowOps<impl ReactiveBorrow<Item = St> + Clone> {
        ReBorrowOps(Rc::new(FilterScan::new(
            initial_state,
            move |state, ctx| {
                let value = self.get(ctx);
                let is_notify = predicate(&state, &value);
                let state = if is_notify { f(state, value) } else { state };
                FilterScanResult { is_notify, state }
            },
            |state| state,
            |state| state,
        )))
    }
    pub fn dedup_by(
        self,
        eq: impl Fn(&S::Item, &S::Item) -> bool + 'static,
    ) -> ReBorrowOps<impl ReactiveBorrow<Item = S::Item> + Clone> {
        ReBorrowOps(Rc::new(FilterScan::new(
            None,
            move |state, ctx| {
                let mut value = self.get(ctx);
                let mut is_notify = false;
                if let Some(old) = state {
                    if eq(&value, &old) {
                        value = old;
                    } else {
                        is_notify = true;
                    }
                }
                FilterScanResult {
                    state: value,
                    is_notify,
                }
            },
            |value| Some(value),
            |value| value,
        )))
    }
    pub fn dedup_by_key<K: PartialEq>(
        self,
        to_key: impl Fn(&S::Item) -> K + 'static,
    ) -> ReBorrowOps<impl ReactiveBorrow<Item = S::Item> + Clone> {
        self.dedup_by(move |l, r| to_key(l) == to_key(r))
    }
    pub fn dedup(self) -> ReBorrowOps<impl ReactiveBorrow<Item = S::Item> + Clone>
    where
        S::Item: PartialEq,
    {
        self.dedup_by(|l, r| l == r)
    }

    pub fn fold<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, S::Item) -> St + 'static,
    ) -> Fold<St> {
        Fold::new(FoldBy::new(
            initial_state,
            move |st, ctx| (f(st, self.get(ctx)), ()),
            |(st, _)| st,
            |st| st,
        ))
    }
    pub fn collect_to<E: Extend<S::Item> + 'static>(self, e: E) -> Fold<E> {
        self.fold(e, |mut e, x| {
            e.extend(once(x));
            e
        })
    }
    pub fn collect<E: Extend<S::Item> + Default + 'static>(self) -> Fold<E> {
        self.collect_to(Default::default())
    }
    pub fn to_vec(self) -> Fold<Vec<S::Item>> {
        self.collect()
    }
    pub fn for_each(self, f: impl FnMut(S::Item) + 'static) -> Subscription {
        self.fold(f, move |mut f, x| {
            f(x);
            f
        })
        .into()
    }
    pub fn for_each_async_with<Fut>(
        self,
        f: impl FnMut(S::Item) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        let mut f = f;
        Fold::new(FoldBy::new(
            (),
            move |_, ctx| ((), sp.spawn_local(f(self.get(ctx)))),
            |_| (),
            |_| (),
        ))
        .into()
    }
    pub fn hot(self) -> ReOps<impl Reactive<Item = S::Item>> {
        ReOps(Hot::new(self))
    }

    pub fn into_stream(self) -> impl futures::Stream<Item = S::Item> {
        IntoStream::new(self)
    }
}
impl<S: Reactive> Reactive for ReOps<S> {
    type Item = S::Item;
    fn get(&self, ctx: &BindContext) -> Self::Item {
        self.0.get(ctx)
    }
    fn into_dyn(self) -> Re<Self::Item>
    where
        Self: Sized,
    {
        self.0.into_dyn()
    }
}

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
pub struct ReBorrowOps<S>(pub S);

impl<S: ReactiveBorrow> ReBorrowOps<S> {
    pub fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, S::Item> {
        self.0.borrow(ctx)
    }
    pub fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &S::Item) -> U) -> U {
        f(ctx, &self.borrow(ctx))
    }
    pub fn ops_ref(self) -> ReRefOps<impl ReactiveRef<Item = S::Item>> {
        struct ReRefByReBorrow<S>(ReBorrowOps<S>);
        impl<S: ReactiveBorrow> ReactiveRef for ReRefByReBorrow<S> {
            type Item = S::Item;
            fn with<U>(
                &self,
                ctx: &BindContext,
                f: impl FnOnce(&BindContext, &Self::Item) -> U,
            ) -> U {
                self.0.with(ctx, f)
            }
            fn into_dyn(self) -> ReRef<Self::Item>
            where
                Self: Sized,
            {
                self.0.into_dyn_ref()
            }
        }
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
pub struct ReRefOps<S>(pub S);

impl<S: ReactiveRef> ReRefOps<S> {
    pub fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &S::Item) -> U) -> U {
        self.0.with(ctx, f)
    }
    pub fn into_dyn(self) -> ReRef<S::Item> {
        self.0.into_dyn()
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

            fn into_dyn(self) -> ReRef<Self::Item>
            where
                Self: Sized,
            {
                self.source.into_dyn().map_borrow()
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
    pub fn to_vec(self) -> Fold<Vec<S::Item>>
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
    fn into_dyn(self) -> ReRef<Self::Item>
    where
        Self: Sized,
    {
        self.0.into_dyn()
    }
}
