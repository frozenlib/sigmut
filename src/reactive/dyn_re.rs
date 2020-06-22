use super::*;

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub struct Re<T: 'static + ?Sized>(pub(super) ReData<T>);

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub(super) enum ReData<T: 'static + ?Sized> {
    Dyn(Rc<dyn DynamicReactive<Item = T>>),
    DynSource(Rc<dyn DynamicReactiveSource<Item = T>>),
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
        re(get).re()
    }
    pub fn constant(value: T) -> Self
    where
        T: Clone,
    {
        re_constant(value).re()
    }

    pub(super) fn from_dyn(inner: impl DynamicReactive<Item = T>) -> Self {
        Self(ReData::Dyn(Rc::new(inner)))
    }

    pub fn as_ref(&self) -> ReRef<T> {
        ReRef(match self.0.clone() {
            ReData::Dyn(rc) => ReRefData::Dyn(rc.as_ref()),
            ReData::DynSource(rc) => ReRefData::DynSource(rc.as_ref()),
        })
    }
    pub fn ops(&self) -> ReOps<Self> {
        ReOps(self.clone())
    }

    pub fn map<U>(&self, f: impl Fn(T) -> U + 'static) -> Re<U> {
        self.ops().map(f).re()
    }
    pub fn flat_map<U>(&self, f: impl Fn(T) -> Re<U> + 'static) -> Re<U> {
        self.ops().flat_map(f).re()
    }
    pub fn map_async_with<Fut>(
        &self,
        f: impl Fn(T) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> ReBorrow<Poll<Fut::Output>>
    where
        Fut: Future + 'static,
    {
        self.ops().map_async_with(f, sp).re_borrow()
    }

    pub fn cached(&self) -> ReBorrow<T> {
        self.ops().cached().re_borrow()
    }
    pub fn scan<St: 'static>(
        &self,
        initial_state: St,
        f: impl Fn(St, T) -> St + 'static,
    ) -> ReBorrow<St> {
        self.ops().scan(initial_state, f).re_borrow()
    }
    pub fn filter_scan<St: 'static>(
        &self,
        initial_state: St,
        predicate: impl Fn(&St, &T) -> bool + 'static,
        f: impl Fn(St, T) -> St + 'static,
    ) -> ReBorrow<St> {
        self.ops()
            .filter_scan(initial_state, predicate, f)
            .re_borrow()
    }

    pub fn dedup_by(&self, eq: impl Fn(&T, &T) -> bool + 'static) -> ReBorrow<T> {
        self.ops().dedup_by(eq).re_borrow()
    }
    pub fn dedup_by_key<K: PartialEq>(&self, to_key: impl Fn(&T) -> K + 'static) -> ReBorrow<T> {
        self.ops().dedup_by_key(to_key).re_borrow()
    }

    pub fn dedup(&self) -> ReBorrow<T>
    where
        T: PartialEq,
    {
        self.ops().dedup().re_borrow()
    }

    pub fn fold<St: 'static>(
        &self,
        initial_state: St,
        f: impl Fn(St, T) -> St + 'static,
    ) -> Fold<St> {
        self.ops().fold(initial_state, f)
    }
    pub fn collect_to<E: Extend<T> + 'static>(&self, e: E) -> Fold<E> {
        self.ops().collect_to(e)
    }
    pub fn collect<E: Extend<T> + Default + 'static>(&self) -> Fold<E> {
        self.ops().collect()
    }
    pub fn collect_vec(&self) -> Fold<Vec<T>> {
        self.ops().collect_vec()
    }

    pub fn for_each(&self, f: impl FnMut(T) + 'static) -> Subscription {
        self.ops().for_each(f)
    }
    pub fn for_each_async_with<Fut>(
        &self,
        f: impl FnMut(T) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        self.ops().for_each_async_with(f, sp)
    }

    pub fn hot(&self) -> Self {
        self.ops().hot().re()
    }

    pub fn stream(&self) -> impl futures::Stream<Item = T> {
        self.ops().stream()
    }
}
impl<T: 'static> Re<Re<T>> {
    pub fn flatten(&self) -> Re<T> {
        self.ops().flatten().re()
    }
}

impl<T> Reactive for Re<T> {
    type Item = T;

    fn get(&self, ctx: &BindContext) -> Self::Item {
        Re::get(self, ctx)
    }
    fn into_re(self) -> Re<Self::Item>
    where
        Self: Sized,
    {
        self
    }
}
