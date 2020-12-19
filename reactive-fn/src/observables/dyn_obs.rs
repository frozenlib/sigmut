use super::*;

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub struct DynObs<T: 'static + ?Sized>(pub(super) DynObsData<T>);

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub(super) enum DynObsData<T: 'static + ?Sized> {
    Dyn(Rc<dyn DynamicObservable<Item = T>>),
    DynSource(Rc<dyn DynamicObservableSource<Item = T>>),
}

impl<T: 'static> DynObs<T> {
    pub fn get(&self, cx: &BindContext) -> T {
        match &self.0 {
            DynObsData::Dyn(rc) => rc.dyn_get(cx),
            DynObsData::DynSource(rc) => rc.clone().dyn_get(cx),
        }
    }
    pub fn head_tail(&self) -> (T, Tail<T>) {
        BindScope::with(|scope| self.head_tail_with(scope))
    }
    pub fn head_tail_with(&self, scope: &BindScope) -> (T, Tail<T>) {
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

    pub(super) fn from_dyn(inner: impl DynamicObservable<Item = T>) -> Self {
        Self(DynObsData::Dyn(Rc::new(inner)))
    }

    pub fn as_ref(&self) -> DynObsRef<T> {
        match self.0.clone() {
            DynObsData::Dyn(rc) => DynObsRef::from_dyn(rc.as_ref()),
            DynObsData::DynSource(rc) => DynObsRef::from_dyn_source(rc.as_ref()),
        }
    }
    pub fn ops(&self) -> ReOps<Self> {
        ReOps(self.clone())
    }

    pub fn map<U>(&self, f: impl Fn(T) -> U + 'static) -> DynObs<U> {
        self.ops().map(f).re()
    }
    pub fn flat_map<U>(&self, f: impl Fn(T) -> DynObs<U> + 'static) -> DynObs<U> {
        self.ops().flat_map(f).re()
    }
    pub fn map_async_with<Fut>(
        &self,
        f: impl Fn(T) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> DynObsBorrow<Poll<Fut::Output>>
    where
        Fut: Future + 'static,
    {
        self.ops().map_async_with(f, sp).re_borrow()
    }

    pub fn cached(&self) -> DynObsBorrow<T> {
        self.ops().cached().re_borrow()
    }
    pub fn scan<St: 'static>(
        &self,
        initial_state: St,
        f: impl Fn(St, T) -> St + 'static,
    ) -> DynObsBorrow<St> {
        self.ops().scan(initial_state, f).re_borrow()
    }
    pub fn filter_scan<St: 'static>(
        &self,
        initial_state: St,
        predicate: impl Fn(&St, &T) -> bool + 'static,
        f: impl Fn(St, T) -> St + 'static,
    ) -> DynObsBorrow<St> {
        self.ops()
            .filter_scan(initial_state, predicate, f)
            .re_borrow()
    }

    pub fn dedup_by(&self, eq: impl Fn(&T, &T) -> bool + 'static) -> DynObsBorrow<T> {
        self.ops().dedup_by(eq).re_borrow()
    }
    pub fn dedup_by_key<K: PartialEq>(&self, to_key: impl Fn(&T) -> K + 'static) -> DynObsBorrow<T> {
        self.ops().dedup_by_key(to_key).re_borrow()
    }

    pub fn dedup(&self) -> DynObsBorrow<T>
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
impl<T: 'static> DynObs<DynObs<T>> {
    pub fn flatten(&self) -> DynObs<T> {
        self.ops().flatten().re()
    }
}

impl<T> Observable for DynObs<T> {
    type Item = T;

    fn get(&self, cx: &BindContext) -> Self::Item {
        DynObs::get(self, cx)
    }
    fn into_re(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        self
    }
}