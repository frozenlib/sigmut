use super::*;
use futures::Future;
use std::{any::Any, borrow::Borrow, cell::Ref, rc::Rc, task::Poll};

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
    pub fn get(&self, cx: &mut BindContext) -> T
    where
        T: Copy,
    {
        *self.borrow(cx)
    }
    pub fn borrow(&self, cx: &mut BindContext) -> Ref<T> {
        match &self.0 {
            DynObsBorrowData::Dyn(rc) => rc.dyn_borrow(cx),
            DynObsBorrowData::DynSource(rc) => rc.dyn_borrow(&rc, cx),
        }
    }
    pub fn with<U>(&self, f: impl FnOnce(&T, &mut BindContext) -> U, cx: &mut BindContext) -> U {
        f(&self.borrow(cx), cx)
    }
    pub fn head(&self) -> Ref<T> {
        BindContext::with_no_sink(|cx| self.borrow(cx))
    }
    pub fn head_tail(&self) -> (Ref<T>, DynTailRef<T>) {
        BindScope::with(|scope| self.head_tail_with(scope))
    }
    pub fn head_tail_with(&self, scope: &BindScope) -> (Ref<T>, DynTailRef<T>) {
        DynTailRef::new_borrow(&self, scope)
    }

    pub fn constant(value: T) -> Self
    where
        T: Sized,
    {
        obs_borrow_constant(value).into_dyn()
    }
    pub fn new<S, F>(this: S, borrow: F) -> Self
    where
        S: 'static,
        for<'a> F: Fn(&'a S, &mut BindContext) -> Ref<'a, T> + 'static,
    {
        obs_borrow(this, borrow).into_dyn()
    }

    pub(crate) fn from_dyn(rc: Rc<dyn DynamicObservableBorrow<Item = T>>) -> Self {
        Self(DynObsBorrowData::Dyn(rc))
    }
    pub(crate) fn from_dyn_source(rc: Rc<dyn DynamicObservableBorrowSource<Item = T>>) -> Self {
        Self(DynObsBorrowData::DynSource(rc))
    }

    pub fn as_ref(&self) -> DynObsRef<T> {
        match self.0.clone() {
            DynObsBorrowData::Dyn(rc) => DynObsRef::from_dyn(rc.as_ref()),
            DynObsBorrowData::DynSource(rc) => DynObsRef::from_dyn_source(rc.as_ref()),
        }
    }
    pub fn obs(&self) -> ObsBorrow<Self> {
        ObsBorrow(self.clone())
    }

    pub fn map<U>(&self, f: impl Fn(&T) -> U + 'static) -> DynObs<U> {
        self.obs().map(f).into_dyn()
    }
    pub fn map_ref<U: ?Sized>(&self, f: impl Fn(&T) -> &U + 'static) -> DynObsBorrow<U> {
        self.obs().map_ref(f).into_dyn()
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
        self.obs().flat_map(f).into_dyn()
    }
    pub fn map_async_with<Fut>(
        &self,
        f: impl Fn(&T) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> DynObsBorrow<Poll<Fut::Output>>
    where
        Fut: Future + 'static,
    {
        self.obs().map_async_with(f, sp).into_dyn()
    }

    pub fn cloned(&self) -> DynObs<T>
    where
        T: Clone,
    {
        self.obs().cloned().into_dyn()
    }

    pub fn scan<St: 'static>(
        &self,
        initial_state: St,
        f: impl Fn(St, &T) -> St + 'static,
    ) -> DynObsBorrow<St> {
        self.obs().scan(initial_state, f).into_dyn()
    }
    pub fn filter_scan<St: 'static>(
        &self,
        initial_state: St,
        predicate: impl Fn(&St, &T) -> bool + 'static,
        f: impl Fn(St, &T) -> St + 'static,
    ) -> DynObsBorrow<St> {
        self.obs()
            .filter_scan(initial_state, predicate, f)
            .into_dyn()
    }

    pub fn fold<St: 'static>(
        &self,
        initial_state: St,
        f: impl Fn(St, &T) -> St + 'static,
    ) -> Fold<St> {
        self.obs().fold(initial_state, f)
    }
    pub fn collect_to<E: for<'a> Extend<&'a T> + 'static>(&self, e: E) -> Fold<E> {
        self.obs().collect_to(e)
    }
    pub fn collect<E: for<'a> Extend<&'a T> + Default + 'static>(&self) -> Fold<E> {
        self.obs().collect()
    }
    pub fn collect_vec(&self) -> Fold<Vec<T>>
    where
        T: Copy,
    {
        self.obs().collect_vec()
    }

    pub fn subscribe(&self, f: impl FnMut(&T) + 'static) -> Subscription {
        self.obs().subscribe(f)
    }
    pub fn subscribe_to<O>(self, o: O) -> DynSubscriber<O>
    where
        for<'a> O: Observer<&'a T>,
    {
        self.as_ref().subscribe_to(o)
    }

    pub fn subscribe_async_with<Fut>(
        &self,
        f: impl FnMut(&T) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        self.obs().subscribe_async_with(f, sp)
    }

    pub fn hot(&self) -> Self {
        self.obs().hot().into_dyn()
    }
}
impl<T: 'static> DynObsBorrow<DynObs<T>> {
    pub fn flatten(&self) -> DynObs<T> {
        self.obs().flatten().into_dyn()
    }
}

impl<T: Copy> Observable for DynObsBorrow<T> {
    type Item = T;
    fn get(&self, cx: &mut BindContext) -> Self::Item {
        DynObsBorrow::get(self, cx)
    }
}
impl<T: ?Sized> ObservableBorrow for DynObsBorrow<T> {
    type Item = T;
    fn borrow(&self, cx: &mut BindContext) -> Ref<Self::Item> {
        DynObsBorrow::borrow(self, cx)
    }
    fn into_dyn(self) -> DynObsBorrow<Self::Item>
    where
        Self: Sized,
    {
        self
    }
}
impl<T: ?Sized> ObservableRef for DynObsBorrow<T> {
    type Item = T;
    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        DynObsBorrow::with(self, f, cx)
    }
}
