use super::*;

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub struct Re<T: 'static + ?Sized>(pub(super) ReData<T>);

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub(super) enum ReData<T: 'static + ?Sized> {
    Dyn(Rc<dyn DynRe<Item = T>>),
    DynSource(Rc<dyn DynReSource<Item = T>>),
}

impl<T: 'static> Re<T> {
    pub fn get(&self, ctx: &BindContext) -> T {
        match &self.0 {
            ReData::Dyn(rc) => rc.dyn_get(ctx),
            ReData::DynSource(rc) => rc.clone().dyn_get(ctx),
        }
    }
    pub fn head_tail(&self, scope: &BindContextScope) -> (T, Tail<T>) {
        Tail::new(self.clone(), scope)
    }

    pub fn new(get: impl Fn(&BindContext) -> T + 'static) -> Self {
        re(get).into_dyn()
    }
    pub fn constant(value: T) -> Self
    where
        T: Clone,
    {
        re_constant(value).into_dyn()
    }

    pub(super) fn from_dyn(inner: impl DynRe<Item = T>) -> Self {
        Self(ReData::Dyn(Rc::new(inner)))
    }

    pub fn ops(&self) -> ReOps<Self> {
        ReOps(self.clone())
    }

    pub fn map<U>(&self, f: impl Fn(T) -> U + 'static) -> Re<U> {
        let this = self.clone();
        Re::new(move |ctx| f(this.get(ctx)))
    }
    pub fn flat_map<U>(&self, f: impl Fn(T) -> Re<U> + 'static) -> Re<U> {
        self.map(f).flatten()
    }
    pub fn map_async_with<Fut>(
        &self,
        f: impl Fn(T) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> ReBorrow<Poll<Fut::Output>>
    where
        Fut: Future + 'static,
    {
        ReBorrow::from_dyn_source(MapAsync::new(self.map(f), sp))
    }

    pub fn cached(&self) -> ReBorrow<T> {
        let this = self.clone();
        ReBorrow::from_dyn_source(Scan::new((), move |_, ctx| this.get(ctx), |_| (), |x| x))
    }
    pub fn scan<St: 'static>(
        &self,
        initial_state: St,
        f: impl Fn(St, T) -> St + 'static,
    ) -> ReBorrow<St> {
        let this = self.clone();
        ReBorrow::from_dyn_source(Scan::new(
            initial_state,
            move |st, ctx| f(st, this.get(ctx)),
            |st| st,
            |st| st,
        ))
    }
    pub fn filter_scan<St: 'static>(
        &self,
        initial_state: St,
        predicate: impl Fn(&St, &T) -> bool + 'static,
        f: impl Fn(St, T) -> St + 'static,
    ) -> ReBorrow<St> {
        let this = self.clone();
        ReBorrow::from_dyn_source(FilterScan::new(
            initial_state,
            move |state, ctx| {
                let value = this.get(ctx);
                let is_notify = predicate(&state, &value);
                let state = if is_notify { f(state, value) } else { state };
                FilterScanResult { is_notify, state }
            },
            |state| state,
            |state| state,
        ))
    }

    pub fn dedup_by(&self, eq: impl Fn(&T, &T) -> bool + 'static) -> ReBorrow<T> {
        let this = self.clone();
        ReBorrow::from_dyn_source(FilterScan::new(
            None,
            move |state, ctx| {
                let mut value = this.get(ctx);
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
        ))
    }
    pub fn dedup_by_key<K: PartialEq>(&self, to_key: impl Fn(&T) -> K + 'static) -> ReBorrow<T> {
        self.dedup_by(move |l, r| to_key(l) == to_key(r))
    }

    pub fn dedup(&self) -> ReBorrow<T>
    where
        T: PartialEq,
    {
        self.dedup_by(|l, r| l == r)
    }

    pub fn fold<St: 'static>(
        &self,
        initial_state: St,
        f: impl Fn(St, T) -> St + 'static,
    ) -> Fold<St> {
        let this = self.clone();
        Fold::new(FoldBy::new(
            initial_state,
            move |st, ctx| (f(st, this.get(ctx)), ()),
            |(st, _)| st,
            |st| st,
        ))
    }
    pub fn collect_to<E: Extend<T> + 'static>(&self, e: E) -> Fold<E> {
        self.fold(e, |mut e, x| {
            e.extend(once(x));
            e
        })
    }
    pub fn collect<E: Extend<T> + Default + 'static>(&self) -> Fold<E> {
        self.collect_to(Default::default())
    }
    pub fn collect_vec(&self) -> Fold<Vec<T>> {
        self.collect()
    }

    pub fn for_each(&self, f: impl FnMut(T) + 'static) -> Subscription {
        self.fold(f, move |mut f, x| {
            f(x);
            f
        })
        .into()
    }
    pub fn for_each_async_with<Fut>(
        &self,
        f: impl FnMut(T) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        let this = self.clone();
        let mut f = f;
        Fold::new(FoldBy::new(
            (),
            move |_, ctx| ((), sp.spawn_local(f(this.get(ctx)))),
            |_| (),
            |_| (),
        ))
        .into()
    }

    pub fn hot(&self) -> Self {
        Self(ReData::Dyn(Hot::new(self.clone())))
    }

    pub fn to_stream(&self) -> impl futures::Stream<Item = T> {
        IntoStream::new(self.clone())
    }

    pub fn as_ref(&self) -> ReRef<T> {
        ReRef::new(self.clone(), |this, ctx, f| f(ctx, &this.get(ctx)))
    }
}
impl<T> Reactive for Re<T> {
    type Item = T;

    fn get(&self, ctx: &BindContext) -> Self::Item {
        Re::get(self, ctx)
    }
    fn into_dyn(self) -> Re<Self::Item>
    where
        Self: Sized,
    {
        self
    }
}
