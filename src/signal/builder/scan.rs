use std::{
    any::Any,
    cell::{Ref, RefCell},
    rc::Rc,
};

use crate::{
    Signal, SignalContext, StateRef,
    core::{
        BindKey, BindSink, BindSource, DirtyLevel, DiscardReaction, NotifyContext, Reaction,
        ReactionContext, SinkBindings, Slot, SourceBinder,
    },
};

use super::{
    Build, DiscardFn, DiscardFnKeep, DiscardFnVoid, DiscardScheduledCell, MapFn, MapFnNone,
    MapFnRaw, ScanBuild, ScanFnBool, ScanFnVoid, SignalNode,
};

#[cfg(test)]
mod tests;

pub(super) fn scan_builder<St>(
    initial_state: St,
    scan: impl ScanFn<St> + 'static,
) -> impl ScanBuild<State = St>
where
    St: 'static,
{
    ScanBuilder {
        initial_state,
        scan,
        discard: DiscardFnKeep,
        map: MapFnNone,
    }
}

pub(super) trait ScanFn<St> {
    const FILTER: bool;
    fn call(&mut self, st: &mut St, sc: &mut SignalContext<'_, '_>) -> bool;
}

pub(super) fn scan_builder_with<St, M>(
    initial_state: St,
    scan: impl ScanFn<St> + 'static,
    discard: impl DiscardFn<St> + 'static,
    map: M,
) -> impl Build<State = M::Output>
where
    St: 'static,
    M: MapFn<St> + 'static,
{
    ScanBuilder {
        initial_state,
        scan,
        discard,
        map,
    }
}

impl<St, F: FnMut(&mut St, &mut SignalContext<'_, '_>)> ScanFn<St> for ScanFnVoid<F> {
    const FILTER: bool = false;
    fn call(&mut self, st: &mut St, sc: &mut SignalContext<'_, '_>) -> bool {
        (self.0)(st, sc);
        true
    }
}

impl<St, F: FnMut(&mut St, &mut SignalContext<'_, '_>) -> bool> ScanFn<St> for ScanFnBool<F> {
    const FILTER: bool = true;
    fn call(&mut self, st: &mut St, sc: &mut SignalContext<'_, '_>) -> bool {
        (self.0)(st, sc)
    }
}

struct ScanBuilder<St, Scan, D, M> {
    initial_state: St,
    scan: Scan,
    discard: D,
    map: M,
}

impl<St, Scan, D> ScanBuild for ScanBuilder<St, Scan, D, MapFnNone>
where
    St: 'static,
    Scan: ScanFn<St> + 'static,
    D: DiscardFn<St> + 'static,
{
    fn on_discard(self, f: impl Fn(&mut Self::State) + 'static) -> impl Build<State = Self::State> {
        ScanBuilder {
            initial_state: self.initial_state,
            scan: self.scan,
            discard: DiscardFnVoid(f),
            map: self.map,
        }
    }
    fn keep(self) -> impl Build<State = Self::State> {
        ScanBuilder {
            initial_state: self.initial_state,
            scan: self.scan,
            discard: DiscardFnKeep,
            map: self.map,
        }
    }
}
impl<St, Scan, D, M> Build for ScanBuilder<St, Scan, D, M>
where
    St: 'static,
    Scan: ScanFn<St> + 'static,
    D: DiscardFn<St> + 'static,
    M: MapFn<St> + 'static,
{
    type State = M::Output;

    fn map_raw<T: ?Sized + 'static>(
        self,
        f: impl for<'a, 'r> Fn(
            StateRef<'a, Self::State>,
            &mut SignalContext<'r, '_>,
            &'a &'r (),
        ) -> StateRef<'a, T>
        + 'static,
    ) -> impl Build<State = T> {
        ScanBuilder {
            initial_state: self.initial_state,
            scan: self.scan,
            discard: self.discard,
            map: MapFnRaw { m: self.map, f },
        }
    }

    fn build(self) -> Signal<Self::State> {
        Signal::from_node(Rc::new_cyclic(|this| ScanNode {
            bindings: ScanBindings {
                sinks: RefCell::new(SinkBindings::new()),
            },
            data: RefCell::new(ScanNodeData {
                state: self.initial_state,
                scan: self.scan,
                sb: SourceBinder::new(this, Slot(0)),
            }),
            discard: self.discard,
            discard_scheduled: Default::default(),
            map: self.map,
        }))
    }
}

struct ScanNodeData<St, Scan> {
    state: St,
    scan: Scan,
    sb: SourceBinder,
}

impl<St, Scan: ScanFn<St>> ScanNodeData<St, Scan> {
    fn update(&mut self, rc: &mut ReactionContext<'_, '_>) -> bool {
        if self.sb.check(rc) {
            self.sb.update(|sc| self.scan.call(&mut self.state, sc), rc)
        } else {
            false
        }
    }
}

// Management shared by every state, scan, discard callback and map type.
struct ScanBindings {
    sinks: RefCell<SinkBindings>,
}

impl ScanBindings {
    fn is_dirty(&self, key: BindKey, rc: &mut ReactionContext<'_, '_>) -> bool {
        self.sinks.borrow().is_dirty(key, rc)
    }

    fn bind(&self, source: Rc<dyn BindSource>, sc: &mut SignalContext<'_, '_>) {
        self.sinks.borrow_mut().bind(source, Slot(0), sc);
    }

    fn unbind(&self, key: BindKey, rc: &mut ReactionContext<'_, '_>) {
        self.sinks.borrow_mut().unbind(key, rc);
    }

    fn rebind(
        &self,
        source: Rc<dyn BindSource>,
        slot: Slot,
        key: BindKey,
        sc: &mut SignalContext<'_, '_>,
    ) {
        self.sinks.borrow_mut().rebind(source, slot, key, sc);
    }

    fn notify(&self, level: DirtyLevel, filter: bool, nc: &mut NotifyContext) {
        self.sinks.borrow_mut().notify(level.maybe_if(filter), nc);
    }

    fn update(&self, is_dirty: bool, rc: &mut ReactionContext<'_, '_>) {
        self.sinks.borrow_mut().update(is_dirty, rc);
    }

    fn try_schedule_discard(&self, scheduled: &impl DiscardScheduledCell) -> bool {
        scheduled.try_schedule(&self.sinks)
    }

    fn start_discard(&self, scheduled: &impl DiscardScheduledCell) -> bool {
        scheduled.reset_schedule();
        self.sinks.borrow().is_empty()
    }
}

struct ScanNode<St, Scan, D, M>
where
    D: DiscardFn<St>,
{
    bindings: ScanBindings,
    data: RefCell<ScanNodeData<St, Scan>>,
    discard: D,
    discard_scheduled: D::ScheduledCell,
    map: M,
}
impl<St, Scan, D, M> SignalNode for ScanNode<St, Scan, D, M>
where
    St: 'static,
    Scan: ScanFn<St> + 'static,
    D: DiscardFn<St> + 'static,
    M: MapFn<St> + 'static,
{
    type Value = M::Output;

    fn borrow<'a, 'r: 'a>(
        &'a self,
        rc_self: Rc<dyn Any>,
        sc: &mut SignalContext<'r, '_>,
    ) -> StateRef<'a, Self::Value> {
        let this = rc_self.downcast::<Self>().unwrap();
        this.watch(sc);
        self.map
            .apply(Ref::map(self.data.borrow(), |x| &x.state).into(), sc)
    }

    fn fmt_debug(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result
    where
        Self::Value: std::fmt::Debug,
    {
        write!(f, "<scan>")
    }
}
impl<St, Scan, D, M> BindSource for ScanNode<St, Scan, D, M>
where
    St: 'static,
    Scan: ScanFn<St> + 'static,
    D: DiscardFn<St> + 'static,
    M: MapFn<St> + 'static,
{
    fn check(self: Rc<Self>, _slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) -> bool {
        self.update(rc);
        self.bindings.is_dirty(key, rc)
    }

    fn unbind(self: Rc<Self>, _slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) {
        self.bindings.unbind(key, rc);
        self.try_schedule_discard(rc);
    }
    fn rebind(self: Rc<Self>, slot: Slot, key: BindKey, sc: &mut SignalContext<'_, '_>) {
        self.bindings.rebind(self.clone(), slot, key, sc);
        self.try_schedule_discard(sc.rc());
    }
}
impl<St, Scan, D, M> BindSink for ScanNode<St, Scan, D, M>
where
    St: 'static,
    Scan: ScanFn<St> + 'static,
    D: DiscardFn<St> + 'static,
    M: MapFn<St> + 'static,
{
    fn notify(self: Rc<Self>, slot: Slot, level: DirtyLevel, nc: &mut NotifyContext) {
        // Release the data borrow before notifying dependants.
        if self.data.borrow_mut().sb.on_notify(slot, level) {
            self.bindings.notify(level, Scan::FILTER, nc);
        }
    }
}

impl<St, Scan, D, M> ScanNode<St, Scan, D, M>
where
    St: 'static,
    Scan: ScanFn<St> + 'static,
    D: DiscardFn<St> + 'static,
    M: MapFn<St> + 'static,
{
    fn update(self: &Rc<Self>, rc: &mut ReactionContext<'_, '_>) {
        if rc.borrow(&self.data).sb.is_clean() {
            return;
        }
        self.try_schedule_discard(rc);
        let mut d = self.data.borrow_mut();
        let is_dirty = d.update(rc);
        self.bindings.update(is_dirty, rc);
    }
    fn watch(self: &Rc<Self>, sc: &mut SignalContext<'_, '_>) {
        self.bindings.bind(self.clone(), sc);
        self.update(sc.rc());
    }
    fn try_schedule_discard(self: &Rc<Self>, rc: &mut ReactionContext<'_, '_>) {
        if self.bindings.try_schedule_discard(&self.discard_scheduled) {
            let reaction = Reaction::from_discard(self.clone());
            rc.schedule_discard(reaction);
        }
    }
}

impl<St, Scan, D, M> DiscardReaction for ScanNode<St, Scan, D, M>
where
    St: 'static,
    Scan: ScanFn<St> + 'static,
    D: DiscardFn<St> + 'static,
    M: MapFn<St> + 'static,
{
    fn discard(self: Rc<Self>, rc: &mut ReactionContext<'_, '_>) {
        if self.bindings.start_discard(&self.discard_scheduled) {
            let mut d = self.data.borrow_mut();
            self.discard.call(&mut d.state);
            d.sb.clear(rc);
        }
    }
}
