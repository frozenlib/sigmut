use super::*;
use std::cell::Ref;
#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub struct DynObsBorrow<T: 'static + ?Sized>(DynObsBorrowData<T>);

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
enum DynObsBorrowData<T: 'static + ?Sized> {
    Dyn(Rc<dyn DynamicObservableBorrow<Item = T>>),
    DynSource(Rc<dyn DynamicObservableBorrowSource<Item = T>>),
}

impl<T: 'static + ?Sized> DynObsBorrow<T> {
    pub fn borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, T> {
        match &self.0 {
            DynObsBorrowData::Dyn(rc) => rc.dyn_borrow(cx),
            DynObsBorrowData::DynSource(rc) => rc.dyn_borrow(&rc, cx),
        }
    }
    pub fn head_tail_with<'a>(&'a self, scope: &'a BindScope) -> (Ref<'a, T>, TailRef<T>) {
        TailRef::new_borrow(&self, scope)
    }

    pub fn constant(value: T) -> Self
    where
        T: Sized,
    {
        re_borrow_constant(value).re_borrow()
    }
    pub fn new<S, F>(this: S, borrow: F) -> Self
    where
        S: 'static,
        for<'a> F: Fn(&'a S, &BindContext<'a>) -> Ref<'a, T> + 'static,
    {
        re_borrow(this, borrow).re_borrow()
    }

    pub(super) fn from_dyn(rc: Rc<dyn DynamicObservableBorrow<Item = T>>) -> Self {
        Self(DynObsBorrowData::Dyn(rc))
    }
    pub(super) fn from_dyn_source(rc: Rc<dyn DynamicObservableBorrowSource<Item = T>>) -> Self {
        Self(DynObsBorrowData::DynSource(rc))
    }

    pub fn as_ref(&self) -> DynObsRef<T> {
        match self.0.clone() {
            DynObsBorrowData::Dyn(rc) => DynObsRef::from_dyn(rc.as_ref()),
            DynObsBorrowData::DynSource(rc) => DynObsRef::from_dyn_source(rc.as_ref()),
        }
    }
    pub fn ops(&self) -> ReBorrowOps<Self> {
        ReBorrowOps(self.clone())
    }

    pub fn map<U>(&self, f: impl Fn(&T) -> U + 'static) -> DynObs<U> {
        self.ops().map(f).re()
    }
    pub fn map_ref<U: ?Sized>(&self, f: impl Fn(&T) -> &U + 'static) -> DynObsBorrow<U> {
        self.ops().map_ref(f).re_borrow()
    }
    pub fn map_borrow<B: ?Sized>(&self) -> DynObsBorrow<B>
    where
        T: Borrow<B>,
    {
        if let Some(b) = Any::downcast_ref::<DynObsBorrow<B>>(self) {
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

    pub fn cloned(&self) -> DynObs<T>
    where
        T: Clone,
    {
        self.ops().cloned().re()
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
        self.ops().hot().re_borrow()
    }
}
impl<T: 'static> DynObsBorrow<DynObs<T>> {
    pub fn flatten(&self) -> DynObs<T> {
        self.ops().flatten().re()
    }
}

impl<T: ?Sized> ObservableBorrow for DynObsBorrow<T> {
    type Item = T;
    fn borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        DynObsBorrow::borrow(self, cx)
    }

    fn into_dyn_borrow(self) -> DynObsBorrow<Self::Item>
    where
        Self: Sized,
    {
        self
    }
}
