use super::{
    from_async::{FnStreamScanOps, FromAsync, FromStreamFn, FromStreamScan},
    stream, Consumed, FnScanOps, Fold, Obs, ObsCallback, ObsSink, Observable, RawHot, RawScan,
    RcObservable, Subscription,
};
use crate::{
    core::{AsyncObsContext, ObsContext},
    utils::into_owned,
};
use derive_ex::derive_ex;
use futures::{Future, Stream};
use std::{borrow::Borrow, iter::once, marker::PhantomData, rc::Rc, task::Poll};

pub trait ObservableBuilder: 'static {
    type Item: ?Sized + 'static;
    type Observable: Observable<Item = Self::Item> + 'static;
    fn build_observable(self) -> Self::Observable;
    fn build_obs(self) -> Obs<Self::Item>;
}

#[derive(Clone)]
pub struct ObsBuilder<B>(pub B);

impl ObsBuilder<()> {
    pub const fn from_obs<T: ?Sized + 'static>(
        o: Obs<T>,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(o)
    }

    pub const fn from_observable<T: ?Sized + 'static>(
        o: impl Observable<Item = T> + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(FromObservable(o))
    }

    pub const fn from_observable_zst<T: ?Sized + 'static>(
        o: impl Observable<Item = T> + Clone + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(FromObservableZst(o))
    }

    pub const fn from_rc_rc<T: ?Sized + 'static>(
        o: Rc<impl RcObservable<Item = T> + 'static>,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(FromRcRc(o))
    }

    pub const fn from_static_ref<T: ?Sized + 'static>(
        value: &'static T,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(FromStaticRef(value))
    }

    pub const fn from_static_get_to<T: ?Sized + 'static>(
        f: impl for<'cb> Fn(ObsSink<'cb, '_, '_, T>) -> Consumed<'cb> + Clone + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(FromObservableZst(FromStaticGetTo {
            f,
            _phantom: PhantomData,
        }))
    }

    pub const fn from_static_get<T: 'static>(
        f: impl Fn(&mut ObsContext) -> T + Clone + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(FromObservableZst(FromStaticGet(f)))
    }

    pub const fn from_get_to<T: ?Sized + 'static>(
        f: impl for<'cb> Fn(ObsSink<'cb, '_, '_, T>) -> Consumed<'cb> + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(FromObservable(FromGetTo {
            f,
            _phantom: PhantomData,
        }))
    }

    pub const fn from_get<T: 'static>(
        f: impl Fn(&mut ObsContext) -> T + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(FromObservable(FromGet(f)))
    }

    pub const fn from_value<T: 'static>(value: T) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(FromObservable(FromValue(value)))
    }
    pub fn from_value_fn<T: 'static>(
        f: impl Fn(&mut ObsContext) -> T + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        Self::from_scan_map(
            None,
            move |st, oc| {
                *st = Some(f(oc));
            },
            |st| st.as_ref().unwrap(),
        )
    }
    pub fn from_scan<St>(
        initial_state: St,
        op: impl Fn(&mut St, &mut ObsContext) + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = St>>
    where
        St: 'static,
    {
        ObsBuilder::from_scan_map(initial_state, op, |st| st)
    }

    pub fn from_scan_map<St, T>(
        initial_state: St,
        op: impl Fn(&mut St, &mut ObsContext) + 'static,
        map: impl Fn(&St) -> &T + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>>
    where
        St: 'static,
        T: ?Sized + 'static,
    {
        let ops = FnScanOps::new(op, |_st| true, map);
        ObsBuilder(FromRcRc(RawScan::new(initial_state, ops, false)))
    }
    pub fn from_scan_filter<St>(
        initial_state: St,
        op: impl Fn(&mut St, &mut ObsContext) -> bool + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = St>>
    where
        St: 'static,
    {
        ObsBuilder::from_scan_filter_map(initial_state, op, |st| st)
    }
    pub fn from_scan_filter_map<St, T>(
        initial_state: St,
        op: impl Fn(&mut St, &mut ObsContext) -> bool + 'static,
        map: impl Fn(&St) -> &T + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>>
    where
        St: 'static,
        T: ?Sized + 'static,
    {
        let ops = FnScanOps::new(op, |_st| false, map);
        ObsBuilder(FromRcRc(RawScan::new(initial_state, ops, false)))
    }

    pub fn from_async<Fut>(
        f: impl Fn(AsyncObsContext) -> Fut + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = Poll<Fut::Output>>>
    where
        Fut: Future + 'static,
        Fut::Output: 'static,
    {
        ObsBuilder(FromRcRc(FromAsync::new(f, false)))
    }

    pub fn from_future<Fut>(
        fut: Fut,
    ) -> ObsBuilder<impl ObservableBuilder<Item = Poll<Fut::Output>>>
    where
        Fut: Future + 'static,
    {
        ObsBuilder::from_stream(::futures::stream::once(fut))
    }
    pub fn from_future_fn<Fut>(
        f: impl Fn(&mut ObsContext) -> Fut + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = Poll<Fut::Output>>>
    where
        Fut: Future + 'static,
    {
        ObsBuilder::from_stream_fn(move |oc| futures::stream::once(f(oc)))
    }

    pub fn from_stream<S>(s: S) -> ObsBuilder<impl ObservableBuilder<Item = Poll<S::Item>>>
    where
        S: Stream + 'static,
    {
        ObsBuilder::from_stream_scan_filter(Poll::Pending, s, |st, value| {
            if let Some(value) = value {
                *st = Poll::Ready(value);
                true
            } else {
                false
            }
        })
    }
    pub fn from_stream_fn<S>(
        f: impl Fn(&mut ObsContext) -> S + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = Poll<S::Item>>>
    where
        S: Stream + 'static,
    {
        ObsBuilder(FromRcRc(FromStreamFn::new(f)))
    }

    pub fn from_stream_scan<St, S, Op>(
        initial_state: St,
        s: S,
        op: Op,
    ) -> ObsBuilder<impl ObservableBuilder<Item = St>>
    where
        St: 'static,
        S: Stream + 'static,
        Op: Fn(&mut St, Option<S::Item>) + 'static,
    {
        ObsBuilder::from_stream_scan_map(initial_state, s, op, |st| st)
    }

    pub fn from_stream_scan_map<T, St, S, Op, Map>(
        initial_state: St,
        s: S,
        op: Op,
        map: Map,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>>
    where
        T: ?Sized + 'static,
        St: 'static,
        S: Stream + 'static,
        Op: Fn(&mut St, Option<S::Item>) + 'static,
        Map: Fn(&St) -> &T + 'static,
    {
        let ops = FnStreamScanOps::new(
            move |st, value| {
                op(st, value);
                true
            },
            map,
        );
        ObsBuilder(FromRcRc(FromStreamScan::new(initial_state, s, ops)))
    }
    pub fn from_stream_scan_filter<St, S, Op>(
        initial_state: St,
        s: S,
        op: Op,
    ) -> ObsBuilder<impl ObservableBuilder<Item = St>>
    where
        St: 'static,
        S: Stream + 'static,
        Op: Fn(&mut St, Option<S::Item>) -> bool + 'static,
    {
        ObsBuilder::from_stream_scan_filter_map(initial_state, s, op, |st| st)
    }
    pub fn from_stream_scan_filter_map<T, St, S, Op, Map>(
        initial_state: St,
        s: S,
        op: Op,
        map: Map,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>>
    where
        T: ?Sized + 'static,
        St: 'static,
        S: Stream + 'static,
        Op: Fn(&mut St, Option<S::Item>) -> bool + 'static,
        Map: Fn(&St) -> &T + 'static,
    {
        let ops = FnStreamScanOps::new(op, map);
        ObsBuilder(FromRcRc(FromStreamScan::new(initial_state, s, ops)))
    }
}
impl<B: ObservableBuilder> ObsBuilder<B> {
    pub fn obs(self) -> Obs<B::Item> {
        self.0.build_obs()
    }
    pub fn observable(self) -> impl Observable<Item = B::Item> {
        self.0.build_observable()
    }

    pub fn map<U: 'static>(
        self,
        f: impl Fn(&B::Item) -> U + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = U>> {
        let o = self.observable();
        ObsBuilder::from_get(move |oc| o.with(|value, _| f(value), oc))
    }
    pub fn map_ref<U: 'static>(
        self,
        f: impl Fn(&B::Item) -> &U + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = U>> {
        let o = self.observable();
        ObsBuilder::from_get_to(move |s| o.with(|value, oc| s.cb.ret(f(value), oc), s.oc))
    }
    pub fn map_future<Fut>(
        self,
        f: impl Fn(&B::Item) -> Fut + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = Poll<Fut::Output>>>
    where
        Fut: Future + 'static,
        B: 'static,
    {
        let o = self.observable();
        ObsBuilder::from_future_fn(move |oc| o.with(|value, _oc| f(value), oc))
    }
    pub fn map_stream<S>(
        self,
        f: impl Fn(&B::Item) -> S + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = Poll<S::Item>>>
    where
        S: Stream + 'static,
        B: 'static,
    {
        let o = self.observable();
        ObsBuilder::from_stream_fn(move |oc| o.with(|value, _oc| f(value), oc))
    }

    pub fn flat_map<U: Observable + 'static>(
        self,
        f: impl Fn(&B::Item) -> U + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = U::Item>> {
        self.map(f).flatten()
    }
    pub fn flat_map_ref<U: Observable + 'static>(
        self,
        f: impl Fn(&B::Item) -> &U + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = U::Item>> {
        self.map_ref(f).flatten()
    }

    pub fn flatten(self) -> ObsBuilder<impl ObservableBuilder<Item = <B::Item as Observable>::Item>>
    where
        B::Item: Observable,
    {
        let o = self.observable();
        ObsBuilder::from_get_to(move |s| o.with(|value, oc| s.cb.context(oc).ret_flat(value), s.oc))
    }

    pub fn scan<St: 'static>(
        self,
        initial_state: St,
        op: impl Fn(&mut St, &B::Item) + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = St>>
    where
        St: 'static,
    {
        self.scan_map(initial_state, op, |st| st)
    }
    pub fn scan_map<St: 'static, T: 'static>(
        self,
        initial_state: St,
        op: impl Fn(&mut St, &B::Item) + 'static,
        map: impl Fn(&St) -> &T + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        let o = self.observable();
        ObsBuilder::from_scan_map(
            initial_state,
            move |st, oc| o.with(|value, _oc| op(st, value), oc),
            map,
        )
    }
    pub fn scan_filter<St: 'static>(
        self,
        initial_state: St,
        op: impl Fn(&mut St, &B::Item) -> bool + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = St>> {
        self.scan_filter_map(initial_state, op, |st| st)
    }
    pub fn scan_filter_map<St: 'static, T: 'static>(
        self,
        initial_state: St,
        op: impl Fn(&mut St, &B::Item) -> bool + 'static,
        map: impl Fn(&St) -> &T + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        let o = self.observable();
        ObsBuilder::from_scan_filter_map(
            initial_state,
            move |st, oc| o.with(|value, _oc| op(st, value), oc),
            map,
        )
    }

    pub fn cached(self) -> ObsBuilder<impl ObservableBuilder<Item = B::Item>>
    where
        B::Item: ToOwned,
    {
        let o = self.observable();
        let ops = FnScanOps::new(
            move |st, oc| {
                if let Some(st) = st {
                    o.with(|value, _oc| value.clone_into(st), oc);
                } else {
                    *st = Some(o.get(oc));
                }
            },
            |st| {
                *st = None;
                true
            },
            |st| st.as_ref().unwrap().borrow(),
        );
        ObsBuilder(FromRcRc(RawScan::new(None, ops, false)))
    }
    pub fn dedup(self) -> ObsBuilder<impl ObservableBuilder<Item = B::Item>>
    where
        B::Item: ToOwned + PartialEq,
    {
        self.dedup_by(|l, r| l == r)
    }
    pub fn dedup_by(
        self,
        eq: impl Fn(&B::Item, &B::Item) -> bool + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = B::Item>>
    where
        B::Item: ToOwned,
    {
        let o = self.observable();
        let ops = FnScanOps::new(
            move |st, oc| {
                if let Some(st) = st {
                    o.with(
                        |value, _oc| {
                            if eq(Borrow::borrow(st), value) {
                                false
                            } else {
                                value.clone_into(st);
                                true
                            }
                        },
                        oc,
                    )
                } else {
                    *st = Some(o.get(oc));
                    true
                }
            },
            |st| {
                *st = None;
                true
            },
            |st| st.as_ref().unwrap().borrow(),
        );
        ObsBuilder(FromRcRc(RawScan::new(
            None::<<B::Item as ToOwned>::Owned>,
            ops,
            false,
        )))
    }
    pub fn dedup_by_key<K>(
        self,
        key: impl Fn(&B::Item) -> K + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = B::Item>>
    where
        B::Item: ToOwned,
        K: PartialEq,
    {
        self.dedup_by(move |l, r| key(l) == key(r))
    }
    pub fn fold<St: 'static>(
        self,
        initial_state: St,
        op: impl Fn(&mut St, &B::Item) + 'static,
    ) -> Fold<St> {
        let o = self.observable();
        Fold::new(initial_state, move |st, oc| {
            o.with(|value, _oc| op(st, value), oc);
        })
    }
    pub fn collect_to<C>(self, collection: C) -> Fold<C>
    where
        B::Item: ToOwned,
        C: Extend<<B::Item as ToOwned>::Owned> + 'static,
    {
        self.fold(collection, |st, value| st.extend(once(value.to_owned())))
    }
    pub fn collect<C>(self) -> Fold<C>
    where
        B::Item: ToOwned,
        C: Extend<<B::Item as ToOwned>::Owned> + Default + 'static,
    {
        self.collect_to(Default::default())
    }
    pub fn collect_vec(self) -> Fold<Vec<<B::Item as ToOwned>::Owned>>
    where
        B::Item: ToOwned,
    {
        self.collect()
    }
    pub fn hot(self) -> ObsBuilder<impl ObservableBuilder<Item = B::Item>> {
        let o = self.observable();
        ObsBuilder(Obs::from_rc(RawHot::new(o)))
    }
    pub fn subscribe(self, mut f: impl FnMut(&B::Item) + 'static) -> Subscription {
        let o = self.observable();
        Subscription::new(move |oc| o.with(|value, _oc| f(value), oc))
    }
    pub fn subscribe_async<Fut>(self, f: impl FnMut(&B::Item) -> Fut + 'static) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        let o = self.observable();
        let mut f = f;
        Subscription::new_async(move |oc| o.with(|value, _oc| f(value), oc))
    }

    pub fn stream(self) -> impl Stream<Item = <B::Item as ToOwned>::Owned> + Unpin + 'static
    where
        B: 'static,
        B::Item: ToOwned + 'static,
    {
        self.stream_map(|value| value.to_owned())
    }
    pub fn stream_map<U: 'static>(
        self,
        f: impl Fn(&B::Item) -> U + 'static,
    ) -> impl Stream<Item = U> + Unpin + 'static {
        let o = self.observable();
        stream(move |oc| o.with(|value, _oc| f(value), oc))
    }
}

struct FromObservable<O>(O);
impl<O: Observable + 'static> ObservableBuilder for FromObservable<O> {
    type Item = O::Item;
    type Observable = O;
    fn build_observable(self) -> Self::Observable {
        self.0
    }
    fn build_obs(self) -> Obs<Self::Item> {
        Obs::from_observable(self.0)
    }
}

struct FromObservableZst<O>(O);
impl<O: Observable + Clone + 'static> ObservableBuilder for FromObservableZst<O> {
    type Item = O::Item;
    type Observable = O;
    fn build_observable(self) -> Self::Observable {
        self.0
    }
    fn build_obs(self) -> Obs<Self::Item> {
        Obs::from_observable_zst(self.0)
    }
}

struct FromRcRc<O>(Rc<O>);
impl<O: RcObservable + 'static> ObservableBuilder for FromRcRc<O> {
    type Item = O::Item;
    type Observable = Rc<O>;
    fn build_observable(self) -> Self::Observable {
        self.0
    }
    fn build_obs(self) -> Obs<Self::Item> {
        Obs::from_rc_rc(self.0)
    }
}

struct FromStaticRef<T: ?Sized + 'static>(&'static T);

impl<T: ?Sized + 'static> ObservableBuilder for FromStaticRef<T> {
    type Item = T;
    type Observable = Self;
    fn build_observable(self) -> Self::Observable {
        self
    }
    fn build_obs(self) -> Obs<Self::Item> {
        Obs::from_static_ref(self.0)
    }
}
impl<T: ?Sized + 'static> Observable for FromStaticRef<T> {
    type Item = T;
    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, oc: &mut ObsContext) -> U {
        f(self.0, oc)
    }
}

#[derive_ex(Clone(bound()))]
struct FromStaticGetTo<F, T>
where
    F: for<'cb> Fn(ObsSink<'cb, '_, '_, T>) -> Consumed<'cb> + Clone + 'static,
    T: ?Sized + 'static,
{
    f: F,
    _phantom: PhantomData<fn(&Self) -> &T>,
}
impl<F, T> Observable for FromStaticGetTo<F, T>
where
    F: for<'cb> Fn(ObsSink<'cb, '_, '_, T>) -> Consumed<'cb> + Clone + 'static,
    T: ?Sized,
{
    type Item = T;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, oc: &mut ObsContext) -> U {
        ObsCallback::with(|cb| self.get_to(cb.context(oc)), f)
    }
    fn get_to<'cb>(&self, s: ObsSink<'cb, '_, '_, Self::Item>) -> super::Consumed<'cb> {
        (self.f)(s)
    }
}

#[derive(Clone)]
struct FromStaticGet<F>(F);
impl<F, T> Observable for FromStaticGet<F>
where
    T: 'static,
    F: Fn(&mut ObsContext) -> T + Clone + 'static,
{
    type Item = T;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, oc: &mut ObsContext) -> U {
        f(&(self.0)(oc), oc)
    }
    fn get(&self, oc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        into_owned((self.0)(oc))
    }
}

struct FromGetTo<F, T: ?Sized> {
    f: F,
    _phantom: PhantomData<fn(&Self) -> &T>,
}

impl<F, T> Observable for FromGetTo<F, T>
where
    F: for<'cb> Fn(ObsSink<'cb, '_, '_, T>) -> Consumed<'cb>,
    T: ?Sized,
{
    type Item = T;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, oc: &mut ObsContext) -> U {
        ObsCallback::with(|cb| self.get_to(cb.context(oc)), f)
    }

    fn get_to<'cb>(&self, s: ObsSink<'cb, '_, '_, Self::Item>) -> super::Consumed<'cb> {
        (self.f)(s)
    }
}

struct FromGet<F>(F);

impl<F, T> Observable for FromGet<F>
where
    F: Fn(&mut ObsContext) -> T,
    T: 'static,
{
    type Item = T;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, oc: &mut ObsContext) -> U {
        f(&(self.0)(oc), oc)
    }
    fn get(&self, oc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        into_owned((self.0)(oc))
    }
}

struct FromValue<T>(T);

impl<T> Observable for FromValue<T> {
    type Item = T;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, oc: &mut ObsContext) -> U {
        f(&self.0, oc)
    }
}
