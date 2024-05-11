use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

use crate::{
    core::{
        waker_from_sink, AsyncSignalContext, AsyncSignalContextSource, BindKey, BindSink,
        BindSource, Dirty, DirtyOrMaybeDirty, Discard, NotifyContext, SinkBindings, Slot,
        SourceBindings, UpdateContext,
    },
    Signal, SignalContext, StateRef,
};

use super::SignalNode;

const SLOT_DEPS: Slot = Slot(0);
const SLOT_WAKE: Slot = Slot(1);

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
    scs: AsyncSignalContextSource,
    fut: Pin<Box<Option<Fut>>>,
    dirty: Dirty,
    is_wake: bool,
    sources: SourceBindings,
    state: St,
    scan: Scan,
    waker: Waker,
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
                scs: AsyncSignalContextSource::new(),
                fut: Box::pin(None),
                dirty: Dirty::Dirty,
                is_wake: false,
                sources: SourceBindings::new(),
                state: initial_state,
                scan,
                waker: waker_from_sink(this.clone(), SLOT_WAKE),
            }),
            map,
            sinks: RefCell::new(SinkBindings::new()),
        })
    }

    fn update(self: &Rc<Self>, uc: &mut UpdateContext) {
        let d = self.data.borrow();
        if d.dirty.is_clean() && !d.is_wake {
            return;
        }
        drop(d);

        let d = &mut *self.data.borrow_mut();
        let mut is_dirty = false;
        if d.dirty.check(&mut d.sources, uc) {
            let sink = Rc::downgrade(self);
            d.fut.set(None);
            d.fut.set(d.sources.update(
                sink,
                SLOT_DEPS,
                true,
                |sc| Some(d.scs.with(sc, || (self.get_fut)(d.scs.sc()))),
                uc,
            ));
            d.dirty = Dirty::Clean;
            d.is_wake = true;
            is_dirty = true;
        }
        let Some(f) = d.fut.as_mut().as_pin_mut() else {
            return;
        };
        let sink = Rc::downgrade(self);
        let value = d.sources.update(
            sink,
            SLOT_DEPS,
            false,
            |sc| {
                d.scs
                    .with(sc, || f.poll(&mut Context::from_waker(&d.waker)))
            },
            uc,
        );
        if value.is_ready() {
            d.fut.set(None);
            is_dirty = true;
        }
        d.is_wake = false;
        if is_dirty {
            let is_dirty = (d.scan)(&mut d.state, value);
            self.sinks.borrow_mut().update(is_dirty, uc);
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
        self.update(sc.uc());
        self.sinks.borrow_mut().bind(self.clone(), Slot(0), sc);
        StateRef::map(
            inner.data.borrow().into(),
            |data| (self.map)(&data.state),
            sc,
        )
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
        if self.sinks.borrow_mut().unbind(key, uc) {
            uc.schedule_discard(self, Slot(0));
        }
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
    fn notify(self: Rc<Self>, slot: Slot, dirty: DirtyOrMaybeDirty, nc: &mut NotifyContext) {
        let mut d = self.data.borrow_mut();
        let mut need_notify = false;
        match slot {
            SLOT_DEPS => {
                need_notify = d.dirty.is_clean();
                d.dirty |= dirty;
            }
            SLOT_WAKE => {
                need_notify = !d.is_wake;
                d.is_wake = true;
            }
            _ => {}
        }
        if need_notify {
            self.sinks
                .borrow_mut()
                .notify(DirtyOrMaybeDirty::MaybeDirty, nc)
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
        if self.sinks.borrow().is_empty() {
            let mut d = self.data.borrow_mut();
            d.fut.set(None);
            d.dirty = Dirty::Dirty;
            d.sources.clear(uc);
        }
    }
}
