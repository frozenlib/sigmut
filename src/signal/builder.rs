use std::{
    cell::{Cell, RefCell},
    future::Future,
};

use futures::Stream;

use crate::{Signal, SignalContext, StateRef, StateRefBuilder, core::SinkBindings};

use self::{
    future_scan::future_scan_builder, get::get_builder, scan::scan_builder,
    stream_scan::stream_scan_builder,
};

use super::SignalNode;

mod future_scan;
mod get;
mod scan;
mod stream_scan;

/// A builder for creating a [`Signal`].
pub struct SignalBuilder<B>(B);

impl SignalBuilder<()> {
    pub fn new<T: 'static>(
        f: impl Fn(&mut SignalContext) -> T + 'static,
    ) -> SignalBuilder<impl GetBuild<State = T>> {
        SignalBuilder(get_builder(f))
    }

    pub fn from_scan<St: 'static>(
        initial_state: St,
        f: impl FnMut(&mut St, &mut SignalContext) + 'static,
    ) -> SignalBuilder<impl ScanBuild<State = St>> {
        SignalBuilder(scan_builder(initial_state, ScanFnVoid(f)))
    }
    pub fn from_scan_filter<St: 'static>(
        initial_state: St,
        f: impl FnMut(&mut St, &mut SignalContext) -> bool + 'static,
    ) -> SignalBuilder<impl ScanBuild<State = St>> {
        SignalBuilder(scan_builder(initial_state, ScanFnBool(f)))
    }
    pub fn from_future_scan<St: 'static, T: 'static>(
        initial_state: St,
        future: impl Future<Output = T> + 'static,
        f: impl FnOnce(&mut St, T) + 'static,
    ) -> SignalBuilder<impl Build<State = St>> {
        SignalBuilder(future_scan_builder(initial_state, future, ScanFnVoid(f)))
    }
    pub fn from_future_scan_filter<St: 'static, T: 'static>(
        initial_state: St,
        future: impl Future<Output = T> + 'static,
        f: impl FnOnce(&mut St, T) -> bool + 'static,
    ) -> SignalBuilder<impl Build<State = St>> {
        SignalBuilder(future_scan_builder(initial_state, future, ScanFnBool(f)))
    }
    pub fn from_stream_scan_filter<St: 'static, I: 'static>(
        initial_state: St,
        stream: impl Stream<Item = I> + 'static,
        f: impl FnMut(&mut St, Option<I>) -> bool + 'static,
    ) -> SignalBuilder<impl Build<State = St>> {
        SignalBuilder(stream_scan_builder(initial_state, stream, f))
    }
}
impl<B: GetBuild> SignalBuilder<B>
where
    B::State: Sized,
{
    pub fn dedup(self) -> SignalBuilder<impl DedupBuild<State = B::State>>
    where
        B::State: PartialEq,
    {
        SignalBuilder(self.0.dedup())
    }
}
impl<B: DedupBuild> SignalBuilder<B>
where
    B::State: Sized,
{
    pub fn on_discard_value(
        self,
        f: impl Fn(B::State) + 'static,
    ) -> SignalBuilder<impl Build<State = B::State>> {
        SignalBuilder(self.0.on_discard_value(f))
    }
}

impl<B: ScanBuild> SignalBuilder<B> {
    pub fn on_discard(
        self,
        f: impl Fn(&mut B::State) + 'static,
    ) -> SignalBuilder<impl Build<State = B::State>> {
        SignalBuilder(self.0.on_discard(f))
    }
    pub fn keep(self) -> SignalBuilder<impl Build<State = B::State>> {
        SignalBuilder(self.0.keep())
    }
}
impl<B: Build> SignalBuilder<B> {
    pub fn map<T: ?Sized + 'static>(
        self,
        f: impl Fn(&B::State) -> &T + 'static,
    ) -> SignalBuilder<impl Build<State = T>> {
        SignalBuilder(self.0.map(f))
    }
    pub fn map_value<T: 'static>(
        self,
        f: impl Fn(&B::State) -> T + 'static,
    ) -> SignalBuilder<impl Build<State = T>> {
        self.map_raw(move |st, sc, _| {
            StateRef::map_ref(st, |st, sc, _| StateRef::from_value(f(st), sc), sc)
        })
    }
    pub fn flat_map<U: ?Sized + 'static>(
        self,
        f: impl Fn(&B::State) -> &Signal<U> + 'static,
    ) -> SignalBuilder<impl Build<State = U>> {
        self.map_raw(move |st, sc, _| {
            StateRefBuilder::from_value_non_static(st, sc)
                .map_ref(|st, sc, _| f(st).borrow(sc))
                .build()
        })
    }

    fn map_raw<T: ?Sized + 'static>(
        self,
        f: impl for<'a, 'r> Fn(
            StateRef<'a, B::State>,
            &mut SignalContext<'r>,
            &'a &'r (),
        ) -> StateRef<'a, T>
        + 'static,
    ) -> SignalBuilder<impl Build<State = T>> {
        SignalBuilder(self.0.map_raw(f))
    }

    pub fn build(self) -> Signal<B::State> {
        self.0.build()
    }
}

pub trait GetBuild: DedupBuild
where
    Self::State: Sized,
{
    fn dedup(self) -> impl DedupBuild<State = Self::State>
    where
        Self::State: PartialEq;
}
pub trait DedupBuild: ScanBuild
where
    Self::State: Sized,
{
    fn on_discard_value(self, f: impl Fn(Self::State) + 'static)
    -> impl Build<State = Self::State>;
}
pub trait ScanBuild: Build {
    fn on_discard(self, f: impl Fn(&mut Self::State) + 'static) -> impl Build<State = Self::State>;
    fn keep(self) -> impl Build<State = Self::State>;
}
pub trait Build: Sized {
    type State: ?Sized + 'static;
    fn map<T: ?Sized + 'static>(
        self,
        f: impl Fn(&Self::State) -> &T + 'static,
    ) -> impl Build<State = T> {
        self.map_raw(move |st, sc, _| StateRef::map(st, &f, sc))
    }
    fn map_raw<T: ?Sized + 'static>(
        self,
        f: impl for<'a, 'r> Fn(
            StateRef<'a, Self::State>,
            &mut SignalContext<'r>,
            &'a &'r (),
        ) -> StateRef<'a, T>
        + 'static,
    ) -> impl Build<State = T>;

    fn build(self) -> Signal<Self::State>;
}

struct ScanFnVoid<F>(F);
struct ScanFnBool<F>(F);

trait DiscardFn<St> {
    type ScheduledCell: DiscardScheduledCell;
    fn call(&self, st: &mut St) -> bool;
}

struct DiscardFnKeep;

impl<St> DiscardFn<St> for DiscardFnKeep {
    type ScheduledCell = ();
    fn call(&self, _: &mut St) -> bool {
        true
    }
}

struct DiscardFnVoid<F>(F);

impl<St, F: Fn(&mut St)> DiscardFn<St> for DiscardFnVoid<F> {
    type ScheduledCell = Cell<bool>;
    fn call(&self, st: &mut St) -> bool {
        (self.0)(st);
        false
    }
}

trait MapFn<Input: ?Sized> {
    type Output: ?Sized;
    fn apply<'a, 'r: 'a>(
        &self,
        input: StateRef<'a, Input>,
        sc: &mut SignalContext<'r>,
    ) -> StateRef<'a, Self::Output>;
}

struct MapFnNone;

impl<Input> MapFn<Input> for MapFnNone
where
    Input: ?Sized,
{
    type Output = Input;

    fn apply<'a, 'r: 'a>(
        &self,
        input: StateRef<'a, Input>,
        _sc: &mut SignalContext<'r>,
    ) -> StateRef<'a, Self::Output> {
        input
    }
}

struct MapFnRaw<M, F> {
    m: M,
    f: F,
}

impl<Input, Output, M, F> MapFn<Input> for MapFnRaw<M, F>
where
    Input: ?Sized + 'static,
    Output: ?Sized + 'static,
    M: MapFn<Input> + 'static,
    F: for<'a, 'r> Fn(
            StateRef<'a, M::Output>,
            &mut SignalContext<'r>,
            &'a &'r (),
        ) -> StateRef<'a, Output>
        + 'static,
{
    type Output = Output;

    fn apply<'a, 'r: 'a>(
        &self,
        input: StateRef<'a, Input>,
        sc: &mut SignalContext<'r>,
    ) -> StateRef<'a, Self::Output> {
        (self.f)(self.m.apply(input, sc), sc, &&())
    }
}

trait DiscardScheduledCell: Default {
    fn try_schedule(&self, sinks: &RefCell<SinkBindings>) -> bool;
    fn reset_schedule(&self);
}
impl DiscardScheduledCell for Cell<bool> {
    fn try_schedule(&self, sinks: &RefCell<SinkBindings>) -> bool {
        if self.get() || !sinks.borrow().is_empty() {
            false
        } else {
            self.set(true);
            true
        }
    }
    fn reset_schedule(&self) {
        self.set(false);
    }
}
impl DiscardScheduledCell for () {
    fn try_schedule(&self, _: &RefCell<SinkBindings>) -> bool {
        false
    }
    fn reset_schedule(&self) {}
}
