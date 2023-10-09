use super::{
    Consumed, DynObservable, Fold, ObsBuilder, ObsCallback, ObsSink, Observable, ObservableBuilder,
    RcObservable, Subscription,
};
use crate::core::{AsyncObsContext, ObsContext};
use derive_ex::derive_ex;
use futures::{Future, Stream};
use reactive_fn_macros::ObservableFmt;
use std::{hash::Hash, mem, ptr, rc::Rc, task::Poll};

trait BoxObservable: DynObservable {
    fn clone_box(&self) -> Box<dyn BoxObservable<Item = Self::Item>>;
}

impl<O> BoxObservable for O
where
    O: Observable + Clone + 'static,
{
    fn clone_box(&self) -> Box<dyn BoxObservable<Item = Self::Item>> {
        Box::new(self.clone())
    }
}

trait DynRcObservable {
    type Item: ?Sized;
    fn rc_get_to<'cb>(self: Rc<Self>, s: ObsSink<'cb, '_, '_, Self::Item>) -> Consumed<'cb>;
    fn rc_get(self: Rc<Self>, oc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned;
}

impl<O: RcObservable> DynRcObservable for O {
    type Item = <Rc<O> as Observable>::Item;

    fn rc_get_to<'cb>(self: Rc<Self>, s: ObsSink<'cb, '_, '_, Self::Item>) -> Consumed<'cb> {
        RcObservable::rc_get_to(&self, s)
    }
    fn rc_get(self: Rc<Self>, oc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        RcObservable::rc_get(&self, oc)
    }
}

enum RawObs<T: ?Sized + 'static> {
    StaticRef(&'static T),
    BoxObs(Box<dyn BoxObservable<Item = T>>),
    RcObs(Rc<dyn DynObservable<Item = T>>),
    RcRcObs(Rc<dyn DynRcObservable<Item = T>>),
}

impl<T: ?Sized + 'static> Clone for RawObs<T> {
    fn clone(&self) -> Self {
        match self {
            Self::StaticRef(value) => Self::StaticRef(value),
            Self::BoxObs(x) => Self::BoxObs(x.clone_box()),
            Self::RcObs(x) => Self::RcObs(x.clone()),
            Self::RcRcObs(x) => Self::RcRcObs(x.clone()),
        }
    }
}

/// A shareable version of [`Observable`].
#[derive_ex(Clone(bound()))]
#[derive(ObservableFmt)]
#[observable_fmt(self_crate, bound(T))]
pub struct Obs<T: ?Sized + 'static>(RawObs<T>);

impl<T: ?Sized + 'static> Obs<T> {
    /// Creates [`ObsBuilder`] from [Obs].
    ///
    /// Using [`ObsBuilder::obs`] and then [`Obs::obs_builder`] does not return to the original [`ObsBuilder`].
    /// For efficient processing, [`ObsBuilder::obs`] should not be called unless final [`Obs`] is required.
    pub fn obs_builder(&self) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder::from_obs(self.clone())
    }

    pub fn new(f: impl Fn(&mut ObsContext) -> T + 'static) -> Self
    where
        T: Sized,
    {
        ObsBuilder::new(f).obs()
    }
    pub fn new_dedup(f: impl Fn(&mut ObsContext) -> T + 'static) -> Self
    where
        T: PartialEq + Sized,
    {
        ObsBuilder::new_dedup(f).obs()
    }
    pub fn new_value(value: T) -> Self
    where
        T: Sized,
    {
        ObsBuilder::new_value(value).obs()
    }

    pub fn from_observable(o: impl Observable<Item = T> + 'static) -> Self {
        Self(RawObs::RcObs(Rc::new(o)))
    }
    pub fn from_observable_zst(o: impl Observable<Item = T> + Copy + 'static) -> Self {
        Self::from_observable_zst_impl(o)
    }
    fn from_observable_zst_impl<O>(o: O) -> Self
    where
        O: Observable<Item = T> + Copy + 'static,
    {
        if mem::size_of::<O>() == 0 {
            Self(RawObs::BoxObs(Box::new(o)))
        } else {
            Self::from_observable(o)
        }
    }

    pub fn from_rc(o: Rc<impl Observable<Item = T> + 'static>) -> Self {
        Self(RawObs::RcObs(o))
    }
    pub fn from_rc_rc(o: Rc<impl RcObservable<Item = T> + 'static>) -> Self {
        Self(RawObs::RcRcObs(o))
    }

    pub const fn from_static_ref(value: &'static T) -> Self {
        Self(RawObs::StaticRef(value))
    }
    pub fn from_static_get_to(
        f: impl for<'cb> Fn(ObsSink<'cb, '_, '_, T>) -> Consumed<'cb> + Copy + 'static,
    ) -> Self {
        ObsBuilder::from_static_get_to(f).obs()
    }
    pub fn from_static_get(f: impl Fn(&mut ObsContext) -> T + Copy + 'static) -> Self
    where
        T: Sized,
    {
        ObsBuilder::from_static_get(f).obs()
    }
    pub fn from_get_to(
        f: impl for<'cb> Fn(ObsSink<'cb, '_, '_, T>) -> Consumed<'cb> + 'static,
    ) -> Self {
        ObsBuilder::from_get_to(f).obs()
    }
    pub fn from_get(f: impl Fn(&mut ObsContext) -> T + 'static) -> Self
    where
        T: Sized,
    {
        ObsBuilder::from_get(f).obs()
    }

    pub fn from_owned<O: std::borrow::Borrow<T> + 'static>(owned: O) -> Self {
        ObsBuilder::from_owned(owned).obs()
    }

    pub fn from_scan(initial_state: T, op: impl Fn(&mut T, &mut ObsContext) + 'static) -> Self
    where
        T: Sized,
    {
        ObsBuilder::from_scan(initial_state, op).obs()
    }
    pub fn from_scan_filter(
        initial_state: T,
        op: impl Fn(&mut T, &mut ObsContext) -> bool + 'static,
    ) -> Self
    where
        T: Sized,
    {
        ObsBuilder::from_scan_filter(initial_state, op).obs()
    }

    pub fn from_async<Fut>(f: impl Fn(AsyncObsContext) -> Fut + 'static) -> Obs<Poll<T>>
    where
        T: Sized,
        Fut: Future<Output = T> + 'static,
    {
        ObsBuilder::from_async(f).obs()
    }

    pub fn from_future(fut: impl Future<Output = T> + 'static) -> Obs<Poll<T>>
    where
        T: Sized,
    {
        ObsBuilder::from_future(fut).obs()
    }
    pub fn from_future_fn<Fut>(f: impl Fn(&mut ObsContext) -> Fut + 'static) -> Obs<Poll<T>>
    where
        T: Sized,
        Fut: Future<Output = T> + 'static,
    {
        ObsBuilder::from_future_fn(f).obs()
    }

    pub fn from_stream(s: impl Stream<Item = T> + 'static) -> Obs<Poll<T>>
    where
        T: Sized,
    {
        ObsBuilder::from_stream(s).obs()
    }
    pub fn from_stream_fn<S>(f: impl Fn(&mut ObsContext) -> S + 'static) -> Obs<Poll<T>>
    where
        T: Sized,
        S: Stream<Item = T> + 'static,
    {
        ObsBuilder::from_stream_fn(f).obs()
    }

    pub fn from_stream_scan<S, Op>(initial_state: T, s: S, op: Op) -> Self
    where
        T: Sized,
        S: Stream + 'static,
        Op: Fn(&mut T, Option<S::Item>) + 'static,
    {
        ObsBuilder::from_stream_scan(initial_state, s, op).obs()
    }
    pub fn from_stream_scan_filter<S, Op>(initial_state: T, s: S, op: Op) -> Self
    where
        T: Sized,
        S: Stream + 'static,
        Op: Fn(&mut T, Option<S::Item>) -> bool + 'static,
    {
        ObsBuilder::from_stream_scan_filter(initial_state, s, op).obs()
    }
    pub fn with<U>(&self, f: impl FnOnce(&T, &mut ObsContext) -> U, oc: &mut ObsContext) -> U {
        if let RawObs::StaticRef(x) = &self.0 {
            f(x, oc)
        } else {
            ObsCallback::with(|cb| self.get_to(cb.context(oc)), f)
        }
    }
    pub fn get_to<'cb>(&self, s: ObsSink<'cb, '_, '_, T>) -> Consumed<'cb> {
        match &self.0 {
            RawObs::StaticRef(value) => s.ret(value),
            RawObs::BoxObs(x) => x.dyn_get_to(s),
            RawObs::RcObs(x) => x.dyn_get_to(s),
            RawObs::RcRcObs(x) => x.clone().rc_get_to(s),
        }
    }

    pub fn get(&self, oc: &mut ObsContext) -> T::Owned
    where
        T: ToOwned,
    {
        match &self.0 {
            RawObs::StaticRef(value) => <T as ToOwned>::to_owned(value),
            RawObs::BoxObs(x) => x.dyn_get(oc),
            RawObs::RcObs(x) => x.dyn_get(oc),
            RawObs::RcRcObs(x) => x.clone().rc_get(oc),
        }
    }

    pub fn map<U: ?Sized>(&self, f: impl Fn(&T) -> &U + 'static) -> Obs<U> {
        self.obs_builder().map(f).obs()
    }
    pub fn map_value<U>(&self, f: impl Fn(&T) -> U + 'static) -> Obs<U> {
        self.obs_builder().map_value(f).obs()
    }
    pub fn map_future<Fut>(self, f: impl Fn(&T) -> Fut + 'static) -> Obs<Poll<Fut::Output>>
    where
        T: Sized,
        Fut: Future<Output = T> + 'static,
    {
        self.obs_builder().map_future(f).obs()
    }
    pub fn map_stream<S>(self, f: impl Fn(&T) -> S + 'static) -> Obs<Poll<S::Item>>
    where
        T: Sized,
        S: Stream + 'static,
    {
        self.obs_builder().map_stream(f).obs()
    }

    pub fn flat_map<U: ?Sized>(&self, f: impl Fn(&T) -> &U + 'static) -> Obs<U::Item>
    where
        U: Observable + 'static,
    {
        self.obs_builder().flat_map(f).obs()
    }
    pub fn flat_map_value<U>(&self, f: impl Fn(&T) -> U + 'static) -> Obs<U::Item>
    where
        U: Observable + 'static,
    {
        self.obs_builder().flat_map_value(f).obs()
    }
    pub fn flatten(&self) -> Obs<<T as Observable>::Item>
    where
        T: Observable,
    {
        self.obs_builder().flatten().obs()
    }

    pub fn scan<St: 'static>(
        &self,
        initial_state: St,
        op: impl Fn(&mut St, &T) + 'static,
    ) -> Obs<St> {
        self.obs_builder().scan(initial_state, op).obs()
    }
    pub fn scan_filter<St>(
        &self,
        initial_state: St,
        op: impl Fn(&mut St, &T) -> bool + 'static,
    ) -> Obs<St> {
        self.obs_builder().scan_filter(initial_state, op).obs()
    }

    pub fn memo(&self) -> Self
    where
        T: ToOwned,
    {
        self.obs_builder().memo().obs()
    }
    pub fn dedup(&self) -> Self
    where
        T: ToOwned + PartialEq,
    {
        self.obs_builder().dedup().obs()
    }
    pub fn dedup_by(&self, eq: impl Fn(&T, &T) -> bool + 'static) -> Self
    where
        T: ToOwned,
    {
        self.obs_builder().dedup_by(eq).obs()
    }
    pub fn dedup_by_key<K>(&self, key: impl Fn(&T) -> K + 'static) -> Self
    where
        T: ToOwned,
        K: PartialEq + 'static,
    {
        self.obs_builder().dedup_by_key(key).obs()
    }
    pub fn fold<St: 'static>(
        &self,
        initial_state: St,
        op: impl Fn(&mut St, &T) + 'static,
    ) -> Fold<St> {
        self.obs_builder().fold(initial_state, op)
    }
    pub fn collect_to<C>(&self, collection: C) -> Fold<C>
    where
        T: ToOwned,
        C: Extend<T::Owned> + 'static,
    {
        self.obs_builder().collect_to(collection)
    }
    pub fn collect<C>(&self) -> Fold<C>
    where
        T: ToOwned,
        C: Extend<T::Owned> + Default + 'static,
    {
        self.obs_builder().collect()
    }
    pub fn collect_vec(&self) -> Fold<Vec<T::Owned>>
    where
        T: ToOwned,
    {
        self.obs_builder().collect_vec()
    }

    /// Return `Obs` that is calculated before other values and does not send a notification saying "caches might be outdated".
    ///
    /// This behavior reduces overhead.
    ///
    /// However, since there is no difference in priority among `Obs` to which `hasty` has been applied,
    /// applying `hasty` to `Obs` that depends on `Obs` to which `hasty` has been applied
    /// may result in calculations being conducted based on an outdated state.
    /// In this case, recalculations using the new state will be performed, which might potentially slow things down.
    pub fn hasty(&self) -> Self {
        self.obs_builder().hasty().obs()
    }

    /// Retrun `Obs` that does not discard caches even when there is no observer.
    ///
    /// If called `Runtime::update` or `Runtime::update_with(true)`,
    /// caches without observers will be immediately discarded.
    ///
    /// `Obs` returned by `keep()` becomes an observer itself,
    /// preventing the cache from being discarded even when there are no other observers.
    ///
    /// Unlike `hot()`, it does not act as a factor to update the value to the latest.
    pub fn keep(&self) -> Self {
        self.obs_builder().keep().obs()
    }
    /// Return `Obs` that is always updated to the latest value.
    ///
    /// Returned `Obs` is updated when `Runtime::update` or `Runtime::update_with` is called.
    pub fn hot(&self) -> Self {
        self.obs_builder().hot().obs()
    }
    pub fn subscribe(&self, f: impl FnMut(&T) + 'static) -> Subscription {
        self.obs_builder().subscribe(f)
    }
    pub fn subscribe_async<Fut>(&self, f: impl FnMut(&T) -> Fut + 'static) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        self.obs_builder().subscribe_async(f)
    }

    pub fn stream(&self) -> impl Stream<Item = <T as ToOwned>::Owned> + Unpin + 'static
    where
        T: ToOwned + 'static,
    {
        self.stream_map(|value| value.to_owned())
    }
    pub fn stream_map<U: 'static>(
        &self,
        f: impl Fn(&T) -> U + 'static,
    ) -> impl Stream<Item = U> + Unpin + 'static {
        self.obs_builder().stream_map(f)
    }
}
impl<T: ?Sized + 'static> Observable for Obs<T> {
    type Item = T;

    fn with<U>(&self, f: impl FnOnce(&T, &mut ObsContext) -> U, oc: &mut ObsContext) -> U {
        Obs::with(self, f, oc)
    }
    fn get_to<'cb>(&self, s: ObsSink<'cb, '_, '_, T>) -> Consumed<'cb> {
        Obs::get_to(self, s)
    }
}

/// Checks if the values are always equal.
///
/// If the current values are equal now but may differ in the future, they are treated as not equal.
///
/// Even if the values are always equal, they are not guaranteed to be treated as equal and may be determined not to be equal.
impl<T: ?Sized> PartialEq for Obs<T> {
    #[allow(clippy::vtable_address_comparisons)]
    fn eq(&self, other: &Self) -> bool {
        if ptr::eq(self, other) {
            true
        } else {
            match (&self.0, &other.0) {
                (RawObs::StaticRef(this), RawObs::StaticRef(other)) => ptr::eq(this, other),
                (RawObs::RcObs(this), RawObs::RcObs(other)) => Rc::ptr_eq(this, other),
                (RawObs::RcRcObs(this), RawObs::RcRcObs(other)) => Rc::ptr_eq(this, other),
                _ => false,
            }
        }
    }
}

impl<T: ?Sized> Hash for Obs<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match &self.0 {
            RawObs::StaticRef(x) => ptr::hash(x, state),
            RawObs::BoxObs(x) => ptr::hash(&**x, state),
            RawObs::RcObs(x) => ptr::hash(&**x, state),
            RawObs::RcRcObs(x) => ptr::hash(&**x, state),
        }
    }
}

impl From<String> for Obs<str> {
    fn from(s: String) -> Self {
        Obs::from_owned(s)
    }
}
