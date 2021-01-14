use super::*;
use futures::Future;
use std::{any::Any, borrow::Borrow, marker::PhantomData, rc::Rc, task::Poll};

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
    pub fn get(&self, cx: &BindContext) -> T
    where
        T: Copy,
    {
        self.with(|value, _| *value, cx)
    }
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

    pub fn head_tail<U>(&self, f: impl FnOnce(&T) -> U) -> (U, DynTailRef<T>) {
        BindScope::with(|scope| self.head_tail_with(scope, f))
    }
    pub fn head_tail_with<U>(
        &self,
        scope: &BindScope,
        f: impl FnOnce(&T) -> U,
    ) -> (U, DynTailRef<T>) {
        if let DynObsRefData::StaticRef(x) = &self.0 {
            (f(x), DynTailRef::empty())
        } else {
            DynTailRef::new(self.clone(), scope, f)
        }
    }
    pub fn new<S: 'static>(
        this: S,
        f: impl Fn(&S, &mut dyn FnMut(&T, &BindContext), &BindContext) + 'static,
    ) -> Self {
        // `DynamicObsRefByFn` is more optimized than `obs_ref(this,f).into_dyn()`.
        Self::from_dyn(Rc::new(DynamicObsRefByFn {
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
    pub fn constant_map<S: 'static>(value: S, f: impl Fn(&S) -> &T + 'static) -> Self {
        Self::new(value, move |value, f_outer, cx| f_outer(f(value), cx))
    }
    pub fn static_ref(value: &'static T) -> Self {
        Self(DynObsRefData::StaticRef(value))
    }

    pub fn obs(&self) -> ObsRef<Self> {
        ObsRef(self.clone())
    }

    pub(crate) fn from_dyn(rc: Rc<dyn DynamicObservableRef<Item = T>>) -> Self {
        Self(DynObsRefData::Dyn(rc))
    }

    pub(crate) fn from_dyn_source(rc: Rc<dyn DynamicObservableRefSource<Item = T>>) -> Self {
        Self(DynObsRefData::DynSource(rc))
    }

    pub fn map<U>(&self, f: impl Fn(&T) -> U + 'static) -> DynObs<U> {
        self.obs().map(f).into_dyn()
    }
    pub fn map_ref<U: ?Sized>(&self, f: impl Fn(&T) -> &U + 'static) -> DynObsRef<U> {
        if let DynObsRefData::StaticRef(x) = &self.0 {
            DynObsRef::static_ref(f(x))
        } else {
            self.obs().map_ref(f).into_dyn()
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

    pub fn cloned(&self) -> DynObs<T>
    where
        T: Clone,
    {
        self.obs().cloned().into_dyn()
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
        self.obs().subscribe_to(o).into_dyn()
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
impl<T: 'static> DynObsRef<DynObs<T>> {
    pub fn flatten(&self) -> DynObs<T> {
        self.obs().flatten().into_dyn()
    }
}
impl<T: Copy> Observable for DynObsRef<T> {
    type Item = T;
    fn get(&self, cx: &BindContext) -> Self::Item {
        DynObsRef::get(self, cx)
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

// `DynamicObsRefByFn` is more optimized than `obs_ref(this,f).into_dyn()`.
struct DynamicObsRefByFn<S, T: ?Sized, F> {
    this: S,
    f: F,
    _phantom: PhantomData<fn(&Self) -> &T>,
}
impl<S, T, F> DynamicObservable for DynamicObsRefByFn<S, T, F>
where
    S: 'static,
    T: 'static + Copy,
    F: Fn(&S, &mut dyn FnMut(&T, &BindContext), &BindContext) + 'static,
{
    type Item = T;
    fn dyn_get(&self, cx: &BindContext) -> T {
        let mut result = None;
        self.dyn_with(&mut |value, _| result = Some(*value), cx);
        result.unwrap()
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = Self::Item>> {
        self
    }
}
impl<S, T, F> DynamicObservableRef for DynamicObsRefByFn<S, T, F>
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