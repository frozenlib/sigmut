use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::Poll,
};

use crate::{
    core::{
        AsyncSignalContext, AsyncSourceBinder, BindKey, BindSink, BindSource, Discard,
        NotifyContext, NotifyLevel, SinkBindings, Slot, UpdateContext,
    },
    Signal, SignalContext, StateRef,
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

    fn update(self: &Rc<Self>, uc: &mut UpdateContext) {
        if uc.borrow(&self.data).asb.is_clean() {
            return;
        }
        self.try_schedule_discard(uc);
        let d = &mut *self.data.borrow_mut();
        let mut is_dirty = false;
        if d.asb.check(uc) {
            d.fut.set(None);
            d.fut.set(Some(d.asb.init(&self.get_fut, uc)));
            is_dirty = true;
        }
        let Some(fut) = d.fut.as_mut().as_pin_mut() else {
            return;
        };
        let value = d.asb.poll(fut, uc);
        if value.is_ready() {
            d.fut.set(None);
            is_dirty = true;
        }
        if is_dirty {
            let is_dirty = (d.scan)(&mut d.state, value);
            self.sinks.borrow_mut().update(is_dirty, uc);
        }
    }
    fn try_schedule_discard(self: &Rc<Self>, uc: &mut UpdateContext) {
        if self.sinks.borrow().is_empty() && !self.discard_scheduled.replace(true) {
            uc.schedule_discard(self.clone(), Slot(0));
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
        self: Rc<Self>,
        inner: &'a Self,
        sc: &mut SignalContext<'s>,
    ) -> StateRef<'a, Self::Value> {
        self.sinks.borrow_mut().bind(self.clone(), Slot(0), sc);
        self.update(sc.uc());
        StateRef::map(
            inner.data.borrow().into(),
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

impl<St, T, GetFut, Fut, Scan, Map> BindSink for ScanAsyncNode<St, GetFut, Fut, Scan, Map>
where
    St: 'static,
    T: ?Sized + 'static,
    GetFut: Fn(AsyncSignalContext) -> Fut + 'static,
    Fut: Future + 'static,
    Scan: FnMut(&mut St, Poll<Fut::Output>) -> bool + 'static,
    Map: Fn(&St) -> &T + 'static,
{
    fn notify(self: Rc<Self>, slot: Slot, level: NotifyLevel, nc: &mut NotifyContext) {
        let mut d = self.data.borrow_mut();
        if d.asb.on_notify(slot, level) {
            self.sinks.borrow_mut().notify(NotifyLevel::MaybeDirty, nc)
        }
    }
}

impl<St, T, GetFut, Fut, Scan, Map> Discard for ScanAsyncNode<St, GetFut, Fut, Scan, Map>
where
    St: 'static,
    T: ?Sized + 'static,
    GetFut: Fn(AsyncSignalContext) -> Fut + 'static,
    Fut: Future + 'static,
    Scan: FnMut(&mut St, Poll<Fut::Output>) -> bool + 'static,
    Map: Fn(&St) -> &T + 'static,
{
    fn discard(self: Rc<Self>, _slot: Slot, uc: &mut UpdateContext) {
        self.discard_scheduled.set(false);
        if self.sinks.borrow().is_empty() {
            let mut d = self.data.borrow_mut();
            d.fut.set(None);
            d.asb.clear(uc);
        }
    }
}
