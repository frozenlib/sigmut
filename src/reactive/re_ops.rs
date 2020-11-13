use super::*;

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
pub struct ReOps<S>(pub(super) S);

impl<S: Reactive> ReOps<S> {
    pub fn get(&self, ctx: &BindContext) -> S::Item {
        self.0.get(ctx)
    }
    pub fn with<T>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &S::Item) -> T) -> T {
        f(ctx, &self.get(ctx))
    }
    pub fn head_tail(self) -> (S::Item, TailOps<S>) {
        BindContextScope::with(|scope| self.head_tail_with(scope))
    }
    pub fn head_tail_with(self, scope: &BindContextScope) -> (S::Item, TailOps<S>) {
        TailOps::new(self.0, scope)
    }

    pub fn as_ref(self) -> ReRefOps<ReRefByRe<S>> {
        ReRefOps(ReRefByRe(self))
    }
    pub fn as_any(self) -> ReOps<Re<S::Item>> {
        ReOps(self.re())
    }
    pub fn re(self) -> Re<S::Item> {
        self.0.into_re()
    }
    pub fn re_ref(self) -> ReRef<S::Item> {
        self.0.into_re().as_ref()
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

    pub fn cached(self) -> ReBorrowOps<impl ReactiveBorrow<Item = S::Item> + Clone> {
        ReBorrowOps(Rc::new(Scan::new(
            (),
            scan_op(move |_, ctx| self.get(ctx), |_| (), |x| x),
        )))
    }
    pub fn scan<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, S::Item) -> St + 'static,
    ) -> ReBorrowOps<impl ReactiveBorrow<Item = St> + Clone> {
        ReBorrowOps(Rc::new(Scan::new(
            initial_state,
            scan_op(move |st, ctx| f(st, self.get(ctx)), |st| st, |st| st),
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
            filter_scan_op(
                move |state, ctx| {
                    let value = self.get(ctx);
                    let is_notify = predicate(&state, &value);
                    let state = if is_notify { f(state, value) } else { state };
                    FilterScanLoad { is_notify, state }
                },
                |state| state,
                |state| state,
            ),
        )))
    }
    pub fn dedup_by(
        self,
        eq: impl Fn(&S::Item, &S::Item) -> bool + 'static,
    ) -> ReBorrowOps<impl ReactiveBorrow<Item = S::Item> + Clone> {
        ReBorrowOps(Rc::new(FilterScan::new(
            None,
            filter_scan_op(
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
                    FilterScanLoad {
                        state: value,
                        is_notify,
                    }
                },
                |value| Some(value),
                |value| value,
            ),
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
            fold_by_op(move |st, ctx| f(st, self.get(ctx)), |st| st, |st| st),
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
    pub fn collect_vec(self) -> Fold<Vec<S::Item>> {
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
            fold_by_op(
                move |_, ctx| ((), sp.spawn_local(f(self.get(ctx)))),
                |_| (),
                |_| (),
            ),
        ))
        .into()
    }
    pub fn hot(self) -> ReOps<impl Reactive<Item = S::Item>> {
        ReOps(Hot::new(self))
    }

    pub fn stream(self) -> impl futures::Stream<Item = S::Item> {
        IntoStream::new(self)
    }
}
impl<S: Reactive> Reactive for ReOps<S> {
    type Item = S::Item;
    fn get(&self, ctx: &BindContext) -> Self::Item {
        self.0.get(ctx)
    }
    fn into_re(self) -> Re<Self::Item>
    where
        Self: Sized,
    {
        self.0.into_re()
    }
}

#[derive(Clone)]
pub struct ReRefByRe<S>(ReOps<S>);
impl<S: Reactive> ReactiveRef for ReRefByRe<S> {
    type Item = S::Item;
    fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &Self::Item) -> U) -> U {
        self.0.with(ctx, f)
    }
    fn into_re_ref(self) -> ReRef<Self::Item>
    where
        Self: Sized,
    {
        self.0.re_ref()
    }
}
