use std::{
    any::Any,
    cell::{Ref, RefCell},
    rc::Rc,
};

use crate::{
    Signal, SignalContext, StateRef,
    core::{
        BindKey, BindSink, BindSource, DirtyLevel, NotifyContext, SinkBindings, Slot, SourceBinder,
        Task, UpdateContext,
    },
};

use super::{
    Build, DiscardFn, DiscardFnKeep, DiscardFnVoid, DiscardScheduledCell, MapFn, MapFnNone,
    MapFnRaw, ScanBuild, ScanFnBool, ScanFnVoid, SignalNode,
};

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
    fn call(&mut self, st: &mut St, sc: &mut SignalContext) -> bool;
}

impl<St, F: FnMut(&mut St, &mut SignalContext)> ScanFn<St> for ScanFnVoid<F> {
    const FILTER: bool = false;
    fn call(&mut self, st: &mut St, sc: &mut SignalContext) -> bool {
        (self.0)(st, sc);
        true
    }
}

impl<St, F: FnMut(&mut St, &mut SignalContext) -> bool> ScanFn<St> for ScanFnBool<F> {
    const FILTER: bool = true;
    fn call(&mut self, st: &mut St, sc: &mut SignalContext) -> bool {
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
        f: impl for<'a, 's> Fn(
            StateRef<'a, Self::State>,
            &mut SignalContext<'s>,
            &'a &'s (),
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
            sinks: RefCell::new(SinkBindings::new()),
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

struct ScanNode<St, Scan, D, M>
where
    D: DiscardFn<St>,
{
    sinks: RefCell<SinkBindings>,
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

    fn borrow<'a, 's: 'a>(
        &'a self,
        rc_self: Rc<dyn Any>,
        sc: &mut SignalContext<'s>,
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
    fn check(self: Rc<Self>, _slot: Slot, key: BindKey, uc: &mut UpdateContext) -> bool {
        self.update(uc);
        self.sinks.borrow().is_dirty(key, uc)
    }

    fn unbind(self: Rc<Self>, _slot: Slot, key: BindKey, uc: &mut UpdateContext) {
        self.sinks.borrow_mut().unbind(key, uc);
        self.try_schedule_discard(uc);
    }
    fn rebind(self: Rc<Self>, slot: Slot, key: BindKey, sc: &mut SignalContext) {
        self.sinks.borrow_mut().rebind(self.clone(), slot, key, sc);
        self.try_schedule_discard(sc.uc());
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
        if self.data.borrow_mut().sb.on_notify(slot, level) {
            self.sinks
                .borrow_mut()
                .notify(level.with_filter(Scan::FILTER), nc)
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
    fn update(self: &Rc<Self>, uc: &mut UpdateContext) {
        if uc.borrow(&self.data).sb.is_clean() {
            return;
        }
        self.try_schedule_discard(uc);
        let d = &mut *self.data.borrow_mut();
        if d.sb.check(uc) {
            let is_dirty = d.sb.update(|sc| d.scan.call(&mut d.state, sc), uc);
            if Scan::FILTER {
                self.sinks.borrow_mut().update(is_dirty, uc);
            }
        }
    }
    fn watch(self: &Rc<Self>, sc: &mut SignalContext) {
        self.sinks.borrow_mut().bind(self.clone(), Slot(0), sc);
        self.update(sc.uc());
    }
    fn try_schedule_discard(self: &Rc<Self>, uc: &mut UpdateContext) {
        if self.discard_scheduled.try_schedule(&self.sinks) {
            let task = Task::from_rc_fn(self.clone(), |this, uc| this.discard(uc));
            uc.schedule_discard(task);
        }
    }

    fn discard(self: &Rc<Self>, uc: &mut UpdateContext) {
        self.discard_scheduled.reset_schedule();
        if self.sinks.borrow().is_empty() {
            let mut d = self.data.borrow_mut();
            if self.discard.call(&mut d.state) {
                d.sb.clear(uc);
            }
        }
    }
}
