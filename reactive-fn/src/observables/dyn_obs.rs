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
    pub fn with<U>(&self, f: impl FnOnce(&T, &BindContext) -> U, cx: &BindContext) -> U {
        f(&self.get(cx), cx)
    }
    pub fn head_tail(&self) -> (T, DynTail<T>) {
        BindScope::with(|scope| self.head_tail_with(scope))
    }
    pub fn head_tail_with(&self, scope: &BindScope) -> (T, DynTail<T>) {
        DynTail::new(self.clone(), scope)
    }

    pub fn new(get: impl Fn(&BindContext) -> T + 'static) -> Self {
        obs(get).into_dyn()
    }
    pub fn constant(value: T) -> Self
    where
        T: Clone,
    {
        obs_constant(value).into_dyn()
    }

    pub(super) fn from_dyn(inner: impl DynamicObservable<Item = T>) -> Self {
        Self(DynObsData::Dyn(Rc::new(inner)))
    }
    pub(super) fn from_dyn_source(rc: Rc<dyn DynamicObservableSource<Item = T>>) -> Self {
        Self(DynObsData::DynSource(rc))
    }

    pub fn as_ref(&self) -> DynObsRef<T> {
        match self.0.clone() {
            DynObsData::Dyn(rc) => DynObsRef::from_dyn(rc.as_ref()),
            DynObsData::DynSource(rc) => DynObsRef::from_dyn_source(rc.as_ref()),
        }
    }
    pub fn ops(&self) -> Obs<Self> {
        Obs(self.clone())
    }

    pub fn map<U>(&self, f: impl Fn(T) -> U + 'static) -> DynObs<U> {
        self.ops().map(f).into_dyn()
    }
    pub fn map_ref<U: ?Sized>(&self, f: impl Fn(&T) -> &U + 'static) -> DynObsRef<U> {
        self.as_ref().map_ref(f)
    }
    pub fn map_borrow<B: ?Sized>(&self) -> DynObsRef<B>
    where
        T: Borrow<B>,
    {
        self.as_ref().map_borrow()
    }
    pub fn flat_map<U>(&self, f: impl Fn(T) -> DynObs<U> + 'static) -> DynObs<U> {
        self.ops().flat_map(f).into_dyn()
    }
    pub fn map_async_with<Fut>(
        &self,
        f: impl Fn(T) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> DynObsBorrow<Poll<Fut::Output>>
    where
        Fut: Future + 'static,
    {
        self.ops().map_async_with(f, sp).into_dyn()
    }

    pub fn cached(&self) -> DynObsBorrow<T> {
        self.ops().cached().into_dyn()
    }
    pub fn scan<St: 'static>(
        &self,
        initial_state: St,
        f: impl Fn(St, T) -> St + 'static,
    ) -> DynObsBorrow<St> {
        self.ops().scan(initial_state, f).into_dyn()
    }
    pub fn filter_scan<St: 'static>(
        &self,
        initial_state: St,
        predicate: impl Fn(&St, &T) -> bool + 'static,
        f: impl Fn(St, T) -> St + 'static,
    ) -> DynObsBorrow<St> {
        self.ops()
            .filter_scan(initial_state, predicate, f)
            .into_dyn()
    }

    pub fn dedup_by(&self, eq: impl Fn(&T, &T) -> bool + 'static) -> DynObsBorrow<T> {
        self.ops().dedup_by(eq).into_dyn()
    }
    pub fn dedup_by_key<K: PartialEq>(
        &self,
        to_key: impl Fn(&T) -> K + 'static,
    ) -> DynObsBorrow<T> {
        self.ops().dedup_by_key(to_key).into_dyn()
    }

    pub fn dedup(&self) -> DynObsBorrow<T>
    where
        T: PartialEq,
    {
        self.ops().dedup().into_dyn()
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

    pub fn subscribe(&self, f: impl FnMut(T) + 'static) -> Subscription {
        self.ops().subscribe(f)
    }
    pub fn subscribe_async_with<Fut>(
        &self,
        f: impl FnMut(T) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        self.ops().subscribe_async_with(f, sp)
    }

    pub fn hot(&self) -> Self {
        self.ops().hot().into_dyn()
    }

    pub fn stream(&self) -> impl futures::Stream<Item = T> {
        self.ops().stream()
    }
}
impl<T: 'static> DynObs<DynObs<T>> {
    pub fn flatten(&self) -> DynObs<T> {
        self.ops().flatten().into_dyn()
    }
}

impl<T> Observable for DynObs<T> {
    type Item = T;

    fn get(&self, cx: &BindContext) -> Self::Item {
        DynObs::get(self, cx)
    }
    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        self
    }
}
impl<T> ObservableRef for DynObs<T> {
    type Item = T;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &BindContext) -> U, cx: &BindContext) -> U {
        DynObs::with(self, f, cx)
    }
}
