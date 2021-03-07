use futures::Stream;

use crate::*;
use std::{any::Any, borrow::Borrow, rc::Rc};

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub struct DynObs<T: 'static + ?Sized>(DynObsData<T>);

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
enum DynObsData<T: 'static + ?Sized> {
    Static(&'static T),
    Dyn(Rc<dyn DynamicObservable<Item = T>>),
    DynInner(Rc<dyn DynamicObservableInner<Item = T>>),
}

impl<T: 'static + ?Sized> DynObs<T> {
    pub(crate) fn from_dyn(rc: Rc<dyn DynamicObservable<Item = T>>) -> Self {
        Self(DynObsData::Dyn(rc))
    }

    pub(crate) fn from_dyn_inner(rc: Rc<dyn DynamicObservableInner<Item = T>>) -> Self {
        Self(DynObsData::DynInner(rc))
    }

    pub fn new(f: impl Fn(&mut BindContext) -> T + 'static) -> Self
    where
        T: Sized,
    {
        obs(f).into_dyn()
    }

    pub fn new_with<S: 'static>(
        this: S,
        f: impl Fn(&S, &mut dyn FnMut(&T, &mut BindContext), &mut BindContext) + 'static,
    ) -> Self {
        obs_with(this, f).into_dyn()
    }
    pub fn new_constant(value: T) -> Self
    where
        T: Sized,
    {
        Self::new_with(value, |value, f, cx| f(value, cx))
    }
    pub fn new_constant_map_ref<S: 'static>(value: S, f: impl Fn(&S) -> &T + 'static) -> Self {
        Self::new_with(value, move |value, f_outer, cx| f_outer(f(value), cx))
    }
    pub fn new_static(value: &'static T) -> Self {
        Self(DynObsData::Static(value))
    }

    pub fn obs(&self) -> Obs<impl Observable<Item = T>> {
        Obs(self.clone())
    }
    pub fn get(&self, cx: &mut BindContext) -> T::Owned
    where
        T: ToOwned,
    {
        self.obs().get(cx)
    }
    pub fn get_head(&self) -> T::Owned
    where
        T: ToOwned,
    {
        self.obs().get_head()
    }
    pub fn get_head_tail(&self) -> (T::Owned, DynTail<T>)
    where
        T: ToOwned,
    {
        self.with_head_tail(|value| value.to_owned())
    }
    pub fn with<U>(&self, f: impl FnOnce(&T, &mut BindContext) -> U, cx: &mut BindContext) -> U {
        if let DynObsData::Static(x) = &self.0 {
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
    fn dyn_with(&self, f: &mut dyn FnMut(&T, &mut BindContext), cx: &mut BindContext) {
        match &self.0 {
            DynObsData::Static(value) => f(value, cx),
            DynObsData::Dyn(x) => x.dyn_with(f, cx),
            DynObsData::DynInner(x) => x.clone().dyn_with(f, cx),
        }
    }

    pub fn with_head<U>(&self, f: impl FnOnce(&T) -> U) -> U {
        BindContext::nul(|cx| self.with(|value, _| f(value), cx))
    }

    pub fn with_head_tail<U>(&self, f: impl FnOnce(&T) -> U) -> (U, DynTail<T>) {
        BindScope::with(|scope| {
            if let DynObsData::Static(x) = &self.0 {
                (f(x), DynTail::empty())
            } else {
                DynTail::new(self.clone(), scope, f)
            }
        })
    }
    pub fn map<U>(&self, f: impl Fn(&T) -> U + 'static) -> DynObs<U> {
        self.obs().map(f).into_dyn()
    }
    pub fn map_ref<U: ?Sized>(&self, f: impl Fn(&T) -> &U + 'static) -> DynObs<U> {
        if let DynObsData::Static(x) = &self.0 {
            DynObs::new_static(f(x))
        } else {
            self.obs().map_ref(f).into_dyn()
        }
    }
    pub fn map_borrow<B: ?Sized>(&self) -> DynObs<B>
    where
        T: Borrow<B>,
    {
        if let Some(b) = Any::downcast_ref::<DynObs<B>>(self) {
            b.clone()
        } else {
            self.map_ref(|x| x.borrow())
        }
    }
    pub fn map_as_ref<U: ?Sized>(&self) -> DynObs<U>
    where
        T: AsRef<U>,
    {
        if let Some(s) = Any::downcast_ref::<DynObs<U>>(self) {
            s.clone()
        } else {
            self.map_ref(|x| x.as_ref())
        }
    }
    pub fn flat_map<U>(&self, f: impl Fn(&T) -> DynObs<U> + 'static) -> DynObs<U> {
        self.obs().flat_map(f).into_dyn()
    }
    pub fn flat_map_ref<U>(&self, f: impl Fn(&T) -> &DynObs<U> + 'static) -> DynObs<U> {
        self.obs().flat_map_ref(f).into_dyn()
    }
    // pub fn map_async_with<Fut>(
    //     &self,
    //     f: impl Fn(&T) -> Fut + 'static,
    //     sp: impl LocalSpawn,
    // ) -> DynObsBorrow<Poll<Fut::Output>>
    // where
    //     Fut: Future + 'static,
    // {
    //     self.obs().map_async_with(f, sp).into_dyn()
    // }

    pub fn scan<St: 'static>(
        &self,
        initial_state: St,
        f: impl Fn(&mut St, &T) + 'static,
    ) -> DynObs<St> {
        self.obs().scan(initial_state, f).into_dyn()
    }
    pub fn scan_map<St, U>(
        self,
        initial_state: St,
        f: impl FnMut(&mut St, &T) + 'static,
        m: impl Fn(&St) -> U + 'static,
    ) -> DynObs<U>
    where
        St: 'static,
        U: 'static,
    {
        self.obs().scan_map(initial_state, f, m).into_dyn()
    }
    pub fn scan_map_ref<St, U>(
        self,
        initial_state: St,
        f: impl FnMut(&mut St, &T) + 'static,
        m: impl Fn(&St) -> &U + 'static,
    ) -> DynObs<U>
    where
        St: 'static,
        U: ?Sized + 'static,
    {
        self.obs().scan_map_ref(initial_state, f, m).into_dyn()
    }
    pub fn cached(self) -> DynObs<<T as ToOwned>::Owned>
    where
        T: ToOwned,
    {
        self.obs().cached().into_dyn()
    }

    pub fn filter_scan<St: 'static>(
        &self,
        initial_state: St,
        predicate: impl Fn(&St, &T) -> bool + 'static,
        f: impl Fn(&mut St, &T) + 'static,
    ) -> DynObs<St> {
        self.obs()
            .filter_scan(initial_state, predicate, f)
            .into_dyn()
    }
    pub fn filter_scan_map<St, U>(
        self,
        initial_state: St,
        predicate: impl Fn(&St, &T) -> bool + 'static,
        f: impl FnMut(&mut St, &T) + 'static,
        m: impl Fn(&St) -> U + 'static,
    ) -> DynObs<U>
    where
        St: 'static,
        U: 'static,
    {
        self.obs()
            .filter_scan_map(initial_state, predicate, f, m)
            .into_dyn()
    }
    pub fn filter_scan_map_ref<St, U>(
        self,
        initial_state: St,
        predicate: impl Fn(&St, &T) -> bool + 'static,
        f: impl FnMut(&mut St, &T) + 'static,
        m: impl Fn(&St) -> &U + 'static,
    ) -> DynObs<U>
    where
        St: 'static,
        U: ?Sized + 'static,
    {
        self.obs()
            .filter_scan_map_ref(initial_state, predicate, f, m)
            .into_dyn()
    }

    pub fn dedup_by(self, eq: impl Fn(&T, &T) -> bool + 'static) -> DynObs<T>
    where
        T: ToOwned,
    {
        self.obs().dedup_by(eq).into_dyn()
    }
    pub fn dedup_by_key<K>(self, to_key: impl Fn(&T) -> K + 'static) -> DynObs<T>
    where
        K: PartialEq + 'static,
        T: ToOwned,
    {
        self.obs().dedup_by_key(to_key).into_dyn()
    }
    pub fn dedup(self) -> DynObs<T>
    where
        T: ToOwned + PartialEq,
    {
        self.obs().dedup().into_dyn()
    }

    pub fn fold<St: 'static>(self, st: St, f: impl FnMut(&mut St, &T) + 'static) -> Fold<St> {
        self.obs().fold(st, f)
    }
    pub fn collect_to<E>(self, e: E) -> Fold<E>
    where
        T: ToOwned,
        E: Extend<<T as ToOwned>::Owned> + 'static,
    {
        self.obs().collect_to(e)
    }
    pub fn collect<E>(self) -> Fold<E>
    where
        T: ToOwned,
        E: Extend<<T as ToOwned>::Owned> + Default + 'static,
    {
        self.obs().collect()
    }
    pub fn collect_vec(self) -> Fold<Vec<<T as ToOwned>::Owned>>
    where
        T: ToOwned,
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

    // pub fn subscribe_async_with<Fut>(
    //     &self,
    //     f: impl FnMut(&T) -> Fut + 'static,
    //     sp: impl LocalSpawn,
    // ) -> Subscription
    // where
    //     Fut: Future<Output = ()> + 'static,
    // {
    //     self.obs().subscribe_async_with(f, sp)
    // }

    pub fn hot(&self) -> Self {
        self.obs().hot().into_dyn()
    }
    pub fn stream(&self) -> impl Stream<Item = <T as ToOwned>::Owned>
    where
        T: ToOwned,
    {
        self.obs().stream()
    }
}
impl<T: 'static> DynObs<DynObs<T>> {
    pub fn flatten(&self) -> DynObs<T> {
        self.obs().flatten().into_dyn()
    }
}
impl<T: ?Sized> Observable for DynObs<T> {
    type Item = T;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        DynObs::with(self, f, cx)
    }
    fn into_dyn(self) -> DynObs<Self::Item> {
        self
    }
}

impl<S: Observable> From<Obs<S>> for DynObs<S::Item> {
    fn from(s: Obs<S>) -> Self {
        s.into_dyn()
    }
}
