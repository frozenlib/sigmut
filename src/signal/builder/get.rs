use std::cell::Cell;

use crate::{Signal, SignalContext, StateRef};

use super::{
    Build, DedupBuild, DiscardFn, DiscardFnKeep, GetBuild, MapFn, ScanBuild,
    scan::{ScanFn, scan_builder_with},
};

pub fn get_builder<T: 'static>(
    get: impl Fn(&mut SignalContext<'_, '_>) -> T + 'static,
) -> impl GetBuild<State = T> {
    GetBuilder(GetFnGet(get))
}

trait GetFn: ScanFn<Option<Self::Output>> + 'static {
    type Output: 'static;
}

struct GetFnGet<F>(F);

impl<T, F> GetFn for GetFnGet<F>
where
    T: 'static,
    F: Fn(&mut SignalContext<'_, '_>) -> T + 'static,
{
    type Output = T;
}

impl<T, F: Fn(&mut SignalContext<'_, '_>) -> T> ScanFn<Option<T>> for GetFnGet<F> {
    const FILTER: bool = false;

    fn call(&mut self, st: &mut Option<T>, sc: &mut SignalContext<'_, '_>) -> bool {
        *st = Some((self.0)(sc));
        true
    }
}

struct GetFnGetDedup<F>(F);

impl<T, F> GetFn for GetFnGetDedup<F>
where
    T: 'static + PartialEq,
    F: Fn(&mut SignalContext<'_, '_>) -> T + 'static,
{
    type Output = T;
}

impl<T, F> ScanFn<Option<T>> for GetFnGetDedup<F>
where
    T: PartialEq,
    F: Fn(&mut SignalContext<'_, '_>) -> T,
{
    const FILTER: bool = true;

    fn call(&mut self, st: &mut Option<T>, sc: &mut SignalContext<'_, '_>) -> bool {
        let value = (self.0)(sc);
        if let Some(old) = st
            && old == &value
        {
            return false;
        }
        *st = Some(value);
        true
    }
}

// These callbacks depend on the value or user callback, never on the getter.
struct DiscardOption;

impl<T> DiscardFn<Option<T>> for DiscardOption {
    type ScheduledCell = Cell<bool>;

    fn call(&self, st: &mut Option<T>) {
        st.take();
    }
}

struct DiscardOptionValue<F>(F);

impl<T, F: Fn(T)> DiscardFn<Option<T>> for DiscardOptionValue<F> {
    type ScheduledCell = Cell<bool>;

    fn call(&self, st: &mut Option<T>) {
        if let Some(st) = st.take() {
            (self.0)(st);
        }
    }
}

struct DiscardOptionMut<F>(F);

impl<T, F: Fn(&mut T)> DiscardFn<Option<T>> for DiscardOptionMut<F> {
    type ScheduledCell = Cell<bool>;

    fn call(&self, st: &mut Option<T>) {
        if let Some(st) = st {
            (self.0)(st);
        }
        st.take();
    }
}

struct MapOption;

impl<T: 'static> MapFn<Option<T>> for MapOption {
    type Output = T;

    fn apply<'a, 'r: 'a>(
        &self,
        input: StateRef<'a, Option<T>>,
        sc: &mut SignalContext<'r, '_>,
    ) -> StateRef<'a, T> {
        StateRef::map(input, |st| st.as_ref().unwrap(), sc)
    }
}

struct GetBuilder<Get>(Get);

impl<T, F> GetBuild for GetBuilder<GetFnGet<F>>
where
    T: 'static,
    F: Fn(&mut SignalContext<'_, '_>) -> T + 'static,
{
    fn dedup(self) -> impl DedupBuild<State = Self::State>
    where
        Self::State: PartialEq,
    {
        GetBuilder(GetFnGetDedup(self.0.0))
    }
}

impl<Get: GetFn> DedupBuild for GetBuilder<Get> {
    fn on_discard_value(
        self,
        f: impl Fn(Self::State) + 'static,
    ) -> impl Build<State = Self::State> {
        scan_builder_with(None, self.0, DiscardOptionValue(f), MapOption)
    }
}

impl<Get: GetFn> ScanBuild for GetBuilder<Get> {
    fn on_discard(self, f: impl Fn(&mut Self::State) + 'static) -> impl Build<State = Self::State> {
        scan_builder_with(None, self.0, DiscardOptionMut(f), MapOption)
    }

    fn keep(self) -> impl Build<State = Self::State> {
        scan_builder_with(None, self.0, DiscardFnKeep, MapOption)
    }
}

impl<Get: GetFn> Build for GetBuilder<Get> {
    type State = Get::Output;

    fn map_raw<T: ?Sized + 'static>(
        self,
        f: impl for<'a, 'r> Fn(
            StateRef<'a, Self::State>,
            &mut SignalContext<'r, '_>,
            &'a &'r (),
        ) -> StateRef<'a, T>
        + 'static,
    ) -> impl Build<State = T> {
        scan_builder_with(None, self.0, DiscardOption, MapOption).map_raw(f)
    }

    fn build(self) -> Signal<Self::State> {
        scan_builder_with(None, self.0, DiscardOption, MapOption).build()
    }
}
