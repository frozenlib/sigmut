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
        DiscardReaction, NotifyContext, Reaction, ReactionContext, SinkBindings, Slot,
    },
};

use super::SignalNode;

#[cfg(test)]
mod tests;

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
    fut: Option<Pin<Box<Fut>>>,
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
                fut: None,
                asb: AsyncSourceBinder::new(this),
                state: initial_state,
                scan,
            }),
            map,
            sinks: RefCell::new(SinkBindings::new()),
            discard_scheduled: Cell::new(false),
        })
    }

    fn update(self: &Rc<Self>, rc: &mut ReactionContext<'_, '_>) {
        if !rc.can_poll() || rc.borrow(&self.data).asb.is_clean() {
            return;
        }
        self.try_schedule_discard(rc);
        let is_dirty = {
            let data = &mut *self.data.borrow_mut();
            // Restore only Pending futures, before invoking the scan callback.
            let mut future = data.fut.take();
            let mut is_dirty = data.asb.check(rc);
            if is_dirty {
                drop(future.take());
                future = Some(Box::pin(data.asb.init(&self.get_fut, rc)));
            }
            let Some(fut) = future.as_mut() else {
                return;
            };
            let value = data.asb.poll(fut.as_mut(), rc);
            if value.is_ready() {
                is_dirty = true;
            } else {
                data.fut = future.take();
            }
            if is_dirty {
                (data.scan)(&mut data.state, value)
            } else {
                false
            }
        };
        if is_dirty {
            self.sinks.borrow_mut().update(true, rc);
        }
    }

    fn try_schedule_discard(self: &Rc<Self>, rc: &mut ReactionContext<'_, '_>) {
        if self.sinks.borrow().is_empty() && !self.discard_scheduled.replace(true) {
            let reaction = Reaction::from_discard(self.clone());
            rc.schedule_discard(reaction);
        }
    }
}

impl<St, GetFut, Fut, Scan, Map> DiscardReaction for ScanAsyncNode<St, GetFut, Fut, Scan, Map>
where
    St: 'static,
    GetFut: Fn(AsyncSignalContext) -> Fut + 'static,
    Fut: Future + 'static,
    Scan: FnMut(&mut St, Poll<Fut::Output>) -> bool + 'static,
    Map: 'static,
{
    fn discard(self: Rc<Self>, rc: &mut ReactionContext<'_, '_>) {
        self.discard_scheduled.set(false);
        if self.sinks.borrow().is_empty() {
            let future;
            let mut d = self.data.borrow_mut();
            future = d.fut.take();
            d.asb.clear(rc);
            drop(d);
            drop(future);
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

    fn borrow<'a, 'r: 'a>(
        &'a self,
        rc_self: Rc<dyn Any>,
        sc: &mut SignalContext<'r, '_>,
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
    fn check(self: Rc<Self>, _slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) -> bool {
        self.update(rc);
        self.sinks.borrow().is_dirty(key, rc)
    }

    fn unbind(self: Rc<Self>, _slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) {
        self.sinks.borrow_mut().unbind(key, rc);
        self.try_schedule_discard(rc);
    }

    fn rebind(self: Rc<Self>, slot: Slot, key: BindKey, sc: &mut SignalContext<'_, '_>) {
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
