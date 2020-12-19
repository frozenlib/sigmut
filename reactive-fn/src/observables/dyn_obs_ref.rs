use super::*;

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub struct DynObsRef<T: 'static + ?Sized>(DynObsRefData<T>);

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
enum DynObsRefData<T: 'static + ?Sized> {
    StaticRef(&'static T),
    Dyn(Rc<dyn DynamicObservableRef<Item = T>>),
    DynSource(Rc<dyn DynamicObservableRefSource<Item = T>>),
}

impl<T: 'static + ?Sized> DynObsRef<T> {
    pub fn with<U>(&self, f: impl FnOnce(&T, &BindContext) -> U, cx: &BindContext) -> U {
        if let DynObsRefData::StaticRef(x) = &self.0 {
            f(x, cx)
        } else {
            let mut output = None;
            let mut f = Some(f);
            self.dyn_with(
                &mut |value, cx| output = Some((f.take().unwrap())(value, cx)),
                cx,
            );
            output.unwrap()
        }
    }
    fn dyn_with(&self, f: &mut dyn FnMut(&T, &BindContext), cx: &BindContext) {
        match &self.0 {
            DynObsRefData::StaticRef(x) => f(x, cx),
            DynObsRefData::Dyn(rc) => rc.dyn_with(f, cx),
            DynObsRefData::DynSource(rc) => rc.clone().dyn_with(f, cx),
        }
    }

    pub fn head_tail(&self, f: impl FnOnce(&T)) -> TailRef<T> {
        BindScope::with(|scope| self.head_tail_with(scope, f))
    }
    pub fn head_tail_with(&self, scope: &BindScope, f: impl FnOnce(&T)) -> TailRef<T> {
        if let DynObsRefData::StaticRef(x) = &self.0 {
            f(x);
            return TailRef::empty();
        }
        TailRef::new(self.clone(), scope, f)
    }
    pub fn new<S: 'static>(
        this: S,
        f: impl Fn(&S, &mut dyn FnMut(&T, &BindContext), &BindContext) + 'static,
    ) -> Self {
        struct ReRefFn<S, T: ?Sized, F> {
            this: S,
            f: F,
            _phantom: PhantomData<fn(&Self) -> &T>,
        }
        impl<S, T, F> DynamicObservableRef for ReRefFn<S, T, F>
        where
            S: 'static,
            T: 'static + ?Sized,
            F: Fn(&S, &mut dyn FnMut(&T, &BindContext), &BindContext) + 'static,
        {
            type Item = T;

            fn dyn_with(&self, f: &mut dyn FnMut(&T, &BindContext), cx: &BindContext) {
                (self.f)(&self.this, f, cx)
            }
        }
        Self::from_dyn(Rc::new(ReRefFn {
            this,
            f,
            _phantom: PhantomData,
        }))
    }
    pub fn constant(value: T) -> Self
    where
        T: Sized,
    {
        Self::new(value, |value, f, cx| f(value, cx))
    }
    pub fn static_ref(value: &'static T) -> Self {
        Self(DynObsRefData::StaticRef(value))
    }

    pub fn ops(&self) -> ReRefOps<Self> {
        ReRefOps(self.clone())
    }

    pub(super) fn from_dyn(rc: Rc<dyn DynamicObservableRef<Item = T>>) -> Self {
        Self(DynObsRefData::Dyn(rc))
    }

    pub(super) fn from_dyn_source(rc: Rc<dyn DynamicObservableRefSource<Item = T>>) -> Self {
        Self(DynObsRefData::DynSource(rc))
    }

    pub fn map<U>(&self, f: impl Fn(&T) -> U + 'static) -> DynObs<U> {
        self.ops().map(f).re()
    }
    pub fn map_ref<U: ?Sized>(&self, f: impl Fn(&T) -> &U + 'static) -> DynObsRef<U> {
        if let DynObsRefData::StaticRef(x) = &self.0 {
            DynObsRef::static_ref(f(x))
        } else {
            self.ops().map_ref(f).re_ref()
        }
    }
    pub fn map_borrow<B: ?Sized>(&self) -> DynObsRef<B>
    where
        T: Borrow<B>,
    {
        if let Some(b) = Any::downcast_ref::<DynObsRef<B>>(self) {
            b.clone()
        } else {
            self.map_ref(|x| x.borrow())
        }
    }

    pub fn flat_map<U>(&self, f: impl Fn(&T) -> DynObs<U> + 'static) -> DynObs<U> {
        self.ops().flat_map(f).re()
    }
    pub fn map_async_with<Fut>(
        &self,
        f: impl Fn(&T) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> DynObsBorrow<Poll<Fut::Output>>
    where
        Fut: Future + 'static,
    {
        self.ops().map_async_with(f, sp).re_borrow()
    }
    pub fn scan<St: 'static>(
        &self,
        initial_state: St,
        f: impl Fn(St, &T) -> St + 'static,
    ) -> DynObsBorrow<St> {
        self.ops().scan(initial_state, f).re_borrow()
    }
    pub fn filter_scan<St: 'static>(
        &self,
        initial_state: St,
        predicate: impl Fn(&St, &T) -> bool + 'static,
        f: impl Fn(St, &T) -> St + 'static,
    ) -> DynObsBorrow<St> {
        self.ops()
            .filter_scan(initial_state, predicate, f)
            .re_borrow()
    }

    pub fn cloned(&self) -> DynObs<T>
    where
        T: Clone,
    {
        self.ops().cloned().re()
    }
    pub fn fold<St: 'static>(
        &self,
        initial_state: St,
        f: impl Fn(St, &T) -> St + 'static,
    ) -> Fold<St> {
        self.ops().fold(initial_state, f)
    }
    pub fn collect_to<E: for<'a> Extend<&'a T> + 'static>(&self, e: E) -> Fold<E> {
        self.ops().collect_to(e)
    }
    pub fn collect<E: for<'a> Extend<&'a T> + Default + 'static>(&self) -> Fold<E> {
        self.ops().collect()
    }
    pub fn collect_vec(&self) -> Fold<Vec<T>>
    where
        T: Copy,
    {
        self.ops().collect_vec()
    }
    pub fn for_each(&self, f: impl FnMut(&T) + 'static) -> Subscription {
        self.ops().for_each(f)
    }
    pub fn for_each_async_with<Fut>(
        &self,
        f: impl FnMut(&T) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        self.ops().for_each_async_with(f, sp)
    }

    pub fn hot(&self) -> Self {
        self.ops().hot().re_ref()
    }
}
impl<T: 'static> DynObsRef<DynObs<T>> {
    pub fn flatten(&self) -> DynObs<T> {
        self.ops().flatten().re()
    }
}
impl<T: ?Sized> ObservableRef for DynObsRef<T> {
    type Item = T;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &BindContext) -> U, cx: &BindContext) -> U {
        DynObsRef::with(self, f, cx)
    }
    fn into_dyn(self) -> DynObsRef<Self::Item>
    where
        Self: Sized,
    {
        self
    }
}

pub trait IntoDynObsRef<T: ?Sized> {
    fn into_dyn_obs_ref(self) -> DynObsRef<T>;
}

impl<T> IntoDynObsRef<T> for &'static T
where
    T: ?Sized + 'static,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        DynObsRef::static_ref(self)
    }
}

impl<T, B> IntoDynObsRef<T> for DynObs<B>
where
    T: ?Sized + 'static,
    B: Borrow<T>,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.as_ref().map_borrow()
    }
}
impl<T> IntoDynObsRef<T> for &DynObs<T>
where
    T: 'static,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.as_ref()
    }
}

impl<T, B> IntoDynObsRef<T> for DynObsRef<B>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.map_borrow()
    }
}

impl<T> IntoDynObsRef<T> for &DynObsRef<T>
where
    T: ?Sized + 'static,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.map_borrow()
    }
}
impl<T, B> IntoDynObsRef<T> for DynObsBorrow<B>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.as_ref().map_borrow()
    }
}
impl<T> IntoDynObsRef<T> for &DynObsBorrow<T>
where
    T: ?Sized + 'static,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.as_ref()
    }
}

impl IntoDynObsRef<str> for String {
    fn into_dyn_obs_ref(self) -> DynObsRef<str> {
        if self.is_empty() {
            DynObsRef::static_ref("")
        } else {
            DynObsRef::constant(self).map_borrow()
        }
    }
}
impl IntoDynObsRef<str> for &DynObs<String> {
    fn into_dyn_obs_ref(self) -> DynObsRef<str> {
        self.as_ref().map_borrow()
    }
}

impl IntoDynObsRef<str> for &DynObsRef<String> {
    fn into_dyn_obs_ref(self) -> DynObsRef<str> {
        self.map_borrow()
    }
}
impl IntoDynObsRef<str> for &DynObsBorrow<String> {
    fn into_dyn_obs_ref(self) -> DynObsRef<str> {
        self.as_ref().map_borrow()
    }
}
