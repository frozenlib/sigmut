use super::{
    from_async::{FnStreamScanOps, FromAsync, FromStreamFn, FromStreamScanBuilder},
    stream, AssignOps, Consumed, DedupAssignOps, FnScanOps, Fold, Mode, Obs, ObsCallback, ObsSink,
    Observable, RcObservable, ScanBuilder, ScanOps, SetMode, Subscription,
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

    type Map<F: Fn(&Self::Item) -> &U + 'static, U: ?Sized + 'static>: ObservableBuilder<Item = U>;
    fn map<F, U>(self, f: F) -> Self::Map<F, U>
    where
        F: Fn(&Self::Item) -> &U + 'static,
        U: ?Sized + 'static;
}

/// Builder to create [`Obs`] or [`Observable`] with emphasis on runtime performance.
///
/// [`Obs`] created by `ObsBuilder` works faster with fewer memory allocations than the method of the same name in [`Obs`].
///
/// However, the code size is larger than the method of the same name in [`Obs`].
#[derive(Clone)]
pub struct ObsBuilder<B>(pub(crate) B);

impl ObsBuilder<()> {
    pub fn new<T: 'static>(
        f: impl Fn(&mut ObsContext) -> T + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        let ops = AssignOps(f);
        ObsBuilder(ScanBuilder::new(None, ops, false))
    }
    pub fn new_dedup<T: 'static>(
        f: impl Fn(&mut ObsContext) -> T + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>>
    where
        T: PartialEq,
    {
        let ops = DedupAssignOps(f);
        ObsBuilder(ScanBuilder::new(None, ops, false))
    }
    pub const fn new_value<T: 'static>(value: T) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(FromObservable {
            o: FromValue(value),
            into_obs: Obs::from_observable,
        })
    }

    pub const fn from_obs<T: ?Sized + 'static>(
        o: Obs<T>,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(FromObservable { o, into_obs: |o| o })
    }

    pub const fn from_observable<T: ?Sized + 'static>(
        o: impl Observable<Item = T> + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(FromObservable {
            o,
            into_obs: Obs::from_observable,
        })
    }

    pub const fn from_observable_zst<T: ?Sized + 'static>(
        o: impl Observable<Item = T> + Copy + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(FromObservable {
            o,
            into_obs: Obs::from_observable_zst,
        })
    }

    pub const fn from_rc_rc<T: ?Sized + 'static>(
        o: Rc<impl RcObservable<Item = T> + 'static>,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(FromObservable {
            o,
            into_obs: Obs::from_rc_rc,
        })
    }

    pub const fn from_static_ref<T: ?Sized + 'static>(
        value: &'static T,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(FromStaticRef(value))
    }

    pub const fn from_static_get_to<T: ?Sized + 'static>(
        f: impl for<'cb> Fn(ObsSink<'cb, '_, '_, T>) -> Consumed<'cb> + Copy + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(FromObservable {
            o: FromStaticGetTo {
                f,
                _phantom: PhantomData,
            },
            into_obs: Obs::from_observable_zst,
        })
    }

    pub const fn from_static_get<T: 'static>(
        f: impl Fn(&mut ObsContext) -> T + Copy + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(FromObservable {
            o: FromStaticGet(f),
            into_obs: Obs::from_observable_zst,
        })
    }

    pub const fn from_get_to<T: ?Sized + 'static>(
        f: impl for<'cb> Fn(ObsSink<'cb, '_, '_, T>) -> Consumed<'cb> + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(FromObservable {
            o: FromGetTo {
                f,
                _phantom: PhantomData,
            },
            into_obs: Obs::from_observable,
        })
    }

    pub const fn from_get<T: 'static>(
        f: impl Fn(&mut ObsContext) -> T + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(FromObservable {
            o: FromGet(f),
            into_obs: Obs::from_observable,
        })
    }

    pub fn from_scan<St>(
        initial_state: St,
        op: impl Fn(&mut St, &mut ObsContext) + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = St>>
    where
        St: 'static,
    {
        let ops = FnScanOps::new(op, |_st| true);
        ObsBuilder(ScanBuilder::new(initial_state, ops, false))
    }

    pub fn from_scan_filter<St>(
        initial_state: St,
        op: impl Fn(&mut St, &mut ObsContext) -> bool + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = St>>
    where
        St: 'static,
    {
        let ops = FnScanOps::new(op, |_st| false);
        ObsBuilder(ScanBuilder::new(initial_state, ops, false))
    }

    pub fn from_async<Fut>(
        f: impl Fn(AsyncObsContext) -> Fut + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = Poll<Fut::Output>>>
    where
        Fut: Future + 'static,
        Fut::Output: 'static,
    {
        ObsBuilder(FromObservable {
            o: FromAsync::new(f, false),
            into_obs: Obs::from_rc_rc,
        })
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
        ObsBuilder(FromObservable {
            o: FromStreamFn::new(f),
            into_obs: Obs::from_rc_rc,
        })
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
        let ops = FnStreamScanOps::new(move |st, value| {
            op(st, value);
            true
        });
        ObsBuilder(FromStreamScanBuilder::new(initial_state, s, ops))
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
        let ops = FnStreamScanOps::new(op);
        ObsBuilder(FromStreamScanBuilder::new(initial_state, s, ops))
    }
}
impl<B: ObservableBuilder> ObsBuilder<B> {
    pub fn obs(self) -> Obs<B::Item> {
        self.0.build_obs()
    }
    pub fn observable(self) -> impl Observable<Item = B::Item> {
        self.0.build_observable()
    }

    pub fn map<U: ?Sized + 'static>(
        self,
        f: impl Fn(&B::Item) -> &U + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = U>> {
        ObsBuilder(self.0.map(f))
    }
    pub fn map_value<U: 'static>(
        self,
        f: impl Fn(&B::Item) -> U + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = U>> {
        let o = self.observable();
        ObsBuilder::from_get(move |oc| o.with(|value, _| f(value), oc))
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
        f: impl Fn(&B::Item) -> &U + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = U::Item>> {
        self.map(f).flatten()
    }
    pub fn flat_map_value<U: Observable + 'static>(
        self,
        f: impl Fn(&B::Item) -> U + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = U::Item>> {
        self.map_value(f).flatten()
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
        let o = self.observable();
        ObsBuilder::from_scan(initial_state, move |st, oc| {
            o.with(|value, _oc| op(st, value), oc)
        })
    }
    pub fn scan_filter<St: 'static>(
        self,
        initial_state: St,
        op: impl Fn(&mut St, &B::Item) -> bool + 'static,
    ) -> ObsBuilder<impl ObservableBuilder<Item = St>> {
        let o = self.observable();
        ObsBuilder::from_scan_filter(initial_state, move |st, oc| {
            o.with(|value, _oc| op(st, value), oc)
        })
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
        )
        .map(|st: &Option<_>| st.as_ref().unwrap().borrow());
        ObsBuilder(ScanBuilder::new(None, ops, false))
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
        )
        .map(|st: &Option<_>| st.as_ref().unwrap().borrow());
        ObsBuilder(ScanBuilder::new(
            None::<<B::Item as ToOwned>::Owned>,
            ops,
            false,
        ))
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
    pub fn fast(self) -> ObsBuilder<impl ObservableBuilder<Item = B::Item>> {
        self.mode(Mode {
            is_flush: true,
            ..Mode::default()
        })
    }
    pub fn keep(self) -> ObsBuilder<impl ObservableBuilder<Item = B::Item>> {
        self.mode(Mode {
            is_keep: true,
            ..Mode::default()
        })
    }
    pub fn hot(self) -> ObsBuilder<impl ObservableBuilder<Item = B::Item>> {
        self.mode(Mode {
            is_hot: true,
            ..Mode::default()
        })
    }
    fn mode(self, mode: Mode) -> ObsBuilder<impl ObservableBuilder<Item = B::Item>> {
        let o = self.observable();
        ObsBuilder::from_obs(Obs::from_rc(SetMode::new(o, mode)))
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
        Subscription::new_future(move |oc| o.with(|value, _oc| f(value), oc))
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

pub(crate) struct FromObservable<O, IntoObs> {
    pub o: O,
    pub into_obs: IntoObs,
}

impl<O, IntoObs> ObservableBuilder for FromObservable<O, IntoObs>
where
    O: Observable + 'static,
    IntoObs: FnOnce(O) -> Obs<O::Item> + 'static,
{
    type Item = O::Item;
    type Observable = O;
    fn build_observable(self) -> Self::Observable {
        self.o
    }
    fn build_obs(self) -> Obs<Self::Item> {
        (self.into_obs)(self.o)
    }

    type Map<F: Fn(&Self::Item) -> &U + 'static, U: ?Sized + 'static> = MapBuilder<Self, F>;
    fn map<F, U>(self, f: F) -> Self::Map<F, U>
    where
        F: Fn(&Self::Item) -> &U + 'static,
        U: ?Sized + 'static,
    {
        MapBuilder { b: self, f }
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

    type Map<F: Fn(&Self::Item) -> &U + 'static, U: ?Sized + 'static> = FromStaticRef<U>;

    fn map<F, U>(self, f: F) -> Self::Map<F, U>
    where
        F: Fn(&Self::Item) -> &U + 'static,
        U: ?Sized + 'static,
    {
        FromStaticRef(f(self.0))
    }
}
impl<T: ?Sized + 'static> Observable for FromStaticRef<T> {
    type Item = T;
    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, oc: &mut ObsContext) -> U {
        f(self.0, oc)
    }
}

#[derive_ex(Copy, Clone, bound())]
struct FromStaticGetTo<F, T>
where
    F: for<'cb> Fn(ObsSink<'cb, '_, '_, T>) -> Consumed<'cb> + Copy + 'static,
    T: ?Sized + 'static,
{
    f: F,
    _phantom: PhantomData<fn(&Self) -> &T>,
}
impl<F, T> Observable for FromStaticGetTo<F, T>
where
    F: for<'cb> Fn(ObsSink<'cb, '_, '_, T>) -> Consumed<'cb> + Copy + 'static,
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

#[derive_ex(Copy, Clone)]
struct FromStaticGet<F>(F);
impl<F, T> Observable for FromStaticGet<F>
where
    T: 'static,
    F: Fn(&mut ObsContext) -> T + Copy + 'static,
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

pub(crate) struct Map<O, F> {
    o: O,
    f: F,
}

impl<O, F, T: ?Sized> Observable for Map<O, F>
where
    O: Observable,
    F: Fn(&O::Item) -> &T,
{
    type Item = T;
    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, oc: &mut ObsContext) -> U {
        self.o.with(|v, oc| f((self.f)(v), oc), oc)
    }
}

pub(crate) struct MapBuilder<B, F> {
    pub b: B,
    pub f: F,
}
impl<B, F, T: ?Sized> ObservableBuilder for MapBuilder<B, F>
where
    B: ObservableBuilder,
    F: Fn(&B::Item) -> &T + 'static,
    T: 'static,
{
    type Item = T;
    type Observable = Map<B::Observable, F>;
    fn build_observable(self) -> Self::Observable {
        Map {
            o: self.b.build_observable(),
            f: self.f,
        }
    }
    fn build_obs(self) -> Obs<Self::Item> {
        Obs::from_observable(self.build_observable())
    }

    type Map<F1: Fn(&Self::Item) -> &U + 'static, U: ?Sized + 'static> = MapBuilder<Self, F1>;
    fn map<F1, U>(self, f: F1) -> Self::Map<F1, U>
    where
        F1: Fn(&Self::Item) -> &U + 'static,
        U: ?Sized + 'static,
    {
        MapBuilder { b: self, f }
    }
}
