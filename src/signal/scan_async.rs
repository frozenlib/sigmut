use std::{
    any::Any,
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::Poll,
};

use crate::{
    Signal, SignalContext, StateRef,
    core::{
        AsyncSignalContext, AsyncSourceBinder, BindKey, BindSink, BindSource, DirtyLevel,
        NotifyContext, Reaction, ReactionContext, SinkBindings, Slot,
    },
};

use super::SignalNode;

pub(crate) fn build_scan_async<St, T, Fut>(
    initial_state: St,
    get_fut: impl Fn(AsyncSignalContext) -> Fut + 'static,
    scan: impl FnMut(&mut St, Poll<Fut::Output>) -> bool + 'static,
    map: impl Fn(&St) -> &T + 'static,
) -> Signal<T>
where
    St: 'static,
    T: ?Sized + 'static,
    Fut: Future + 'static,
{
    Signal::from_node(ScanAsyncNode::new(initial_state, get_fut, scan, map))
}

struct ScanAsyncNodeData<St, Fut, Scan> {
    fut: Pin<Box<Option<Fut>>>,
    asb: AsyncSourceBinder,
    state: St,
    scan: Scan,
}

struct ScanAsyncNode<St, GetFut, Fut, Scan, Map>
where
    GetFut: Fn(AsyncSignalContext) -> Fut + 'static,
    Fut: Future,
    Scan: FnMut(&mut St, Poll<Fut::Output>) -> bool + 'static,
{
    get_fut: GetFut,
    data: RefCell<ScanAsyncNodeData<St, Fut, Scan>>,
    map: Map,
    sinks: RefCell<SinkBindings>,
    discard_scheduled: Cell<bool>,
}
impl<St, T, GetFut, Fut, Scan, Map> ScanAsyncNode<St, GetFut, Fut, Scan, Map>
where
    St: 'static,
    T: ?Sized + 'static,
    GetFut: Fn(AsyncSignalContext) -> Fut + 'static,
    Fut: Future + 'static,
    Scan: FnMut(&mut St, Poll<Fut::Output>) -> bool + 'static,
    Map: Fn(&St) -> &T + 'static,
{
    fn new(initial_state: St, get_fut: GetFut, scan: Scan, map: Map) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            get_fut,
            data: RefCell::new(ScanAsyncNodeData {
                fut: Box::pin(None),
                asb: AsyncSourceBinder::new(this),
                state: initial_state,
                scan,
            }),
            map,
            sinks: RefCell::new(SinkBindings::new()),
            discard_scheduled: Cell::new(false),
        })
    }

    fn update(self: &Rc<Self>, rc: &mut ReactionContext) {
        if rc.borrow(&self.data).asb.is_clean() {
            return;
        }
        self.try_schedule_discard(rc);
        let d = &mut *self.data.borrow_mut();
        let mut is_dirty = false;
        if d.asb.check(rc) {
            d.fut.set(None);
            d.fut.set(Some(d.asb.init(&self.get_fut, rc)));
            is_dirty = true;
        }
        let Some(fut) = d.fut.as_mut().as_pin_mut() else {
            return;
        };
        let value = d.asb.poll(fut, rc);
        if value.is_ready() {
            d.fut.set(None);
            is_dirty = true;
        }
        if is_dirty {
            let is_dirty = (d.scan)(&mut d.state, value);
            self.sinks.borrow_mut().update(is_dirty, rc);
        }
    }
    fn try_schedule_discard(self: &Rc<Self>, rc: &mut ReactionContext) {
        if self.sinks.borrow().is_empty() && !self.discard_scheduled.replace(true) {
            let reaction = Reaction::from_rc_fn(self.clone(), |this, rc| this.discard(rc));
            rc.schedule_discard(reaction);
        }
    }

    fn discard(self: &Rc<Self>, rc: &mut ReactionContext) {
        self.discard_scheduled.set(false);
        if self.sinks.borrow().is_empty() {
            let mut d = self.data.borrow_mut();
            d.fut.set(None);
            d.asb.clear(rc);
        }
    }
}
impl<St, T, GetFut, Fut, Scan, Map> SignalNode for ScanAsyncNode<St, GetFut, Fut, Scan, Map>
where
    St: 'static,
    T: ?Sized + 'static,
    GetFut: Fn(AsyncSignalContext) -> Fut + 'static,
    Fut: Future + 'static,
    Scan: FnMut(&mut St, Poll<Fut::Output>) -> bool + 'static,
    Map: Fn(&St) -> &T + 'static,
{
    type Value = T;

    fn borrow<'a, 's: 'a>(
        &'a self,
        rc_self: Rc<dyn Any>,
        sc: &mut SignalContext<'s>,
    ) -> StateRef<'a, Self::Value> {
        let this = rc_self.clone().downcast::<Self>().unwrap();
        self.sinks.borrow_mut().bind(this.clone(), Slot(0), sc);
        this.update(sc.rc());
        StateRef::map(
            self.data.borrow().into(),
            |data| (self.map)(&data.state),
            sc,
        )
    }

    fn fmt_debug(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result
    where
        Self::Value: std::fmt::Debug,
    {
        write!(f, "<async>")
    }
}
impl<St, T, GetFut, Fut, Scan, Map> BindSource for ScanAsyncNode<St, GetFut, Fut, Scan, Map>
where
    St: 'static,
    T: ?Sized + 'static,
    GetFut: Fn(AsyncSignalContext) -> Fut + 'static,
    Fut: Future + 'static,
    Scan: FnMut(&mut St, Poll<Fut::Output>) -> bool + 'static,
    Map: Fn(&St) -> &T + 'static,
{
    fn check(self: Rc<Self>, _slot: Slot, key: BindKey, rc: &mut ReactionContext) -> bool {
        self.update(rc);
        self.sinks.borrow().is_dirty(key, rc)
    }

    fn unbind(self: Rc<Self>, _slot: Slot, key: BindKey, rc: &mut ReactionContext) {
        self.sinks.borrow_mut().unbind(key, rc);
        self.try_schedule_discard(rc);
    }

    fn rebind(self: Rc<Self>, slot: Slot, key: BindKey, sc: &mut SignalContext) {
        self.sinks.borrow_mut().rebind(self.clone(), slot, key, sc);
        self.try_schedule_discard(sc.rc());
    }
}

impl<St, T, GetFut, Fut, Scan, Map> BindSink for ScanAsyncNode<St, GetFut, Fut, Scan, Map>
where
    St: 'static,
    T: ?Sized + 'static,
    GetFut: Fn(AsyncSignalContext) -> Fut + 'static,
    Fut: Future + 'static,
    Scan: FnMut(&mut St, Poll<Fut::Output>) -> bool + 'static,
    Map: Fn(&St) -> &T + 'static,
{
    fn notify(self: Rc<Self>, slot: Slot, level: DirtyLevel, nc: &mut NotifyContext) {
        let mut d = self.data.borrow_mut();
        if d.asb.on_notify(slot, level) {
            self.sinks.borrow_mut().notify(DirtyLevel::MaybeDirty, nc)
        }
    }
}
