use std::{
    cell::{Ref, RefCell},
    rc::Rc,
};

use crate::{
    core::{
        BindKey, BindSink, BindSource, Dirty, DirtyOrMaybeDirty, Discard, NotifyContext,
        SinkBindings, Slot, SourceBindings, UpdateContext,
    },
    Signal, SignalContext, StateRef,
};

use super::{
    Build, DiscardFn, DiscardFnKeep, DiscardFnVoid, MapFn, MapFnNone, MapFnRaw, ScanBuild,
    ScanFnBool, ScanFnVoid, SignalNode,
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
    fn discard(self, f: impl Fn(&mut Self::State) + 'static) -> impl Build<State = Self::State> {
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
        Signal::from_node(Rc::new(ScanNode {
            sinks: RefCell::new(SinkBindings::new()),
            data: RefCell::new(ScanNodeData {
                state: self.initial_state,
                dirty: Dirty::Dirty,
                scan: self.scan,
                sources: SourceBindings::new(),
            }),
            discard: self.discard,
            map: self.map,
        }))
    }
}

struct ScanNodeData<St, Scan> {
    state: St,
    dirty: Dirty,
    scan: Scan,
    sources: SourceBindings,
}

struct ScanNode<St, Scan, D, M> {
    sinks: RefCell<SinkBindings>,
    data: RefCell<ScanNodeData<St, Scan>>,
    discard: D,
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
        self: Rc<Self>,
        inner: &'a Self,
        sc: &mut SignalContext<'s>,
    ) -> StateRef<'a, Self::Value> {
        self.watch(sc);
        self.map
            .apply(Ref::map(inner.data.borrow(), |x| &x.state).into(), sc)
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
        if self.sinks.borrow_mut().unbind(key, uc) {
            uc.schedule_discard(self, Slot(0))
        }
    }
}
impl<St, Scan, D, M> BindSink for ScanNode<St, Scan, D, M>
where
    St: 'static,
    Scan: ScanFn<St> + 'static,
    D: DiscardFn<St> + 'static,
    M: MapFn<St> + 'static,
{
    fn notify(self: Rc<Self>, _slot: Slot, dirty: DirtyOrMaybeDirty, uc: &mut NotifyContext) {
        let mut data = self.data.borrow_mut();
        if data.dirty.is_clean() {
            self.sinks
                .borrow_mut()
                .notify(dirty.with_filter(Scan::FILTER), uc)
        }
        data.dirty |= dirty;
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
        if self.data.borrow().dirty.is_clean() {
            return;
        }
        let d = &mut *self.data.borrow_mut();
        if d.dirty.check(&mut d.sources, uc) {
            let sink = Rc::downgrade(self);
            let is_dirty =
                d.sources
                    .update(sink, Slot(0), true, |sc| d.scan.call(&mut d.state, sc), uc);
            if Scan::FILTER {
                self.sinks.borrow_mut().update(is_dirty, uc);
            }
            d.dirty = Dirty::Clean;
        }
    }
    fn watch(self: &Rc<Self>, sc: &mut SignalContext) {
        self.update(sc.uc());
        self.sinks.borrow_mut().bind(self.clone(), Slot(0), sc);
    }
}
impl<St, Scan, D, M> Discard for ScanNode<St, Scan, D, M>
where
    St: 'static,
    Scan: ScanFn<St> + 'static,
    D: DiscardFn<St> + 'static,
    M: MapFn<St> + 'static,
{
    fn discard(self: Rc<Self>, _slot: Slot, uc: &mut UpdateContext) {
        if !self.sinks.borrow().is_empty() {
            return;
        }
        let mut data = self.data.borrow_mut();
        if self.discard.call(&mut data.state) {
            let data = &mut *self.data.borrow_mut();
            data.sources.clear(uc);
            data.dirty = Dirty::Dirty;
        }
    }
}
