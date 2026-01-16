use std::{
    any::Any,
    cell::{Ref, RefCell},
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

use futures::Future;

use crate::{
    Signal, SignalContext, StateRef,
    core::{
        BindKey, BindSink, BindSource, DirtyLevel, NotifyContext, SinkBindings, Slot,
        UpdateContext, waker_from_sink,
    },
};

use super::{Build, MapFn, MapFnNone, MapFnRaw, ScanFnBool, ScanFnVoid, SignalNode};

pub fn future_scan_builder<St, I>(
    initial_state: St,
    future: impl Future<Output = I> + 'static,
    scan: impl ScanFn<St, I> + 'static,
) -> impl Build<State = St>
where
    St: 'static,
    I: 'static,
{
    FutureScanBuilder {
        initial_state,
        future: Box::pin(future),
        scan,
        map: MapFnNone,
    }
}

pub(super) trait ScanFn<St, T> {
    const FILTER: bool;

    fn call(self, state: &mut St, value: T) -> bool;
}

impl<St, F, T> ScanFn<St, T> for ScanFnVoid<F>
where
    F: FnOnce(&mut St, T),
{
    const FILTER: bool = false;
    fn call(self, state: &mut St, value: T) -> bool {
        self.0(state, value);
        true
    }
}
impl<St, F, T> ScanFn<St, T> for ScanFnBool<F>
where
    F: FnOnce(&mut St, T) -> bool,
{
    const FILTER: bool = true;
    fn call(self, state: &mut St, value: T) -> bool {
        self.0(state, value)
    }
}

struct FutureScanBuilder<St, I, Scan, Map> {
    initial_state: St,
    future: Pin<Box<dyn Future<Output = I>>>,
    scan: Scan,
    map: Map,
}
impl<St, I, Scan, Map> Build for FutureScanBuilder<St, I, Scan, Map>
where
    St: 'static,
    I: 'static,
    Scan: ScanFn<St, I> + 'static,
    Map: MapFn<St> + 'static,
{
    type State = Map::Output;

    fn map_raw<T: ?Sized + 'static>(
        self,
        f: impl for<'a, 's> Fn(
            StateRef<'a, Self::State>,
            &mut SignalContext<'s>,
            &'a &'s (),
        ) -> StateRef<'a, T>
        + 'static,
    ) -> impl Build<State = T> {
        FutureScanBuilder {
            initial_state: self.initial_state,
            future: self.future,
            scan: self.scan,
            map: MapFnRaw { m: self.map, f },
        }
    }

    fn build(self) -> Signal<Self::State> {
        Signal::from_node(FutureScanNode::new(
            self.initial_state,
            self.future,
            self.scan,
            self.map,
        ))
    }
}

struct FutureScanNodeTask<I, Scan> {
    future: Pin<Box<dyn Future<Output = I>>>,
    is_wake: bool,
    f: Scan,
    waker: Waker,
}

struct FutureScanNodeData<St, I, Scan> {
    state: St,
    task: Option<FutureScanNodeTask<I, Scan>>,
}
impl<St, I, Scan> FutureScanNodeData<St, I, Scan> {
    fn is_wake(&self) -> bool {
        if let Some(task) = &self.task {
            task.is_wake
        } else {
            false
        }
    }
}

struct FutureScanNode<St, I, Scan, Map> {
    sinks: RefCell<SinkBindings>,
    data: RefCell<FutureScanNodeData<St, I, Scan>>,
    map: Map,
}

impl<St, I, Scan, Map> FutureScanNode<St, I, Scan, Map>
where
    St: 'static,
    I: 'static,
    Scan: ScanFn<St, I> + 'static,
    Map: MapFn<St> + 'static,
{
    fn new(
        initial_state: St,
        stream: Pin<Box<dyn Future<Output = I>>>,
        f: Scan,
        map: Map,
    ) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            sinks: RefCell::new(SinkBindings::new()),
            data: RefCell::new(FutureScanNodeData {
                state: initial_state,
                task: Some(FutureScanNodeTask {
                    future: stream,
                    is_wake: true,
                    f,
                    waker: waker_from_sink(this.clone(), Slot(0)),
                }),
            }),
            map,
        })
    }

    fn update(self: &Rc<Self>, uc: &mut UpdateContext) {
        if !uc.borrow(&self.data).is_wake() {
            return;
        }
        let d = &mut *self.data.borrow_mut();
        let t = d.task.as_mut().unwrap();
        let is_dirty = if let Poll::Ready(value) =
            t.future.as_mut().poll(&mut Context::from_waker(&t.waker))
        {
            let t = d.task.take().unwrap();
            t.f.call(&mut d.state, value)
        } else {
            t.is_wake = false;
            false
        };
        self.sinks.borrow_mut().update(is_dirty, uc);
    }
}
impl<St, I, Scan, Map> SignalNode for FutureScanNode<St, I, Scan, Map>
where
    St: 'static,
    I: 'static,
    Scan: ScanFn<St, I> + 'static,
    Map: MapFn<St> + 'static,
{
    type Value = Map::Output;
    fn borrow<'a, 's: 'a>(
        &'a self,
        rc_self: Rc<dyn Any>,
        sc: &mut SignalContext<'s>,
    ) -> StateRef<'a, Self::Value> {
        let this = rc_self.clone().downcast::<Self>().unwrap();
        this.update(sc.uc());
        self.sinks.borrow_mut().bind(this, Slot(0), sc);
        self.map
            .apply(Ref::map(self.data.borrow(), |data| &data.state).into(), sc)
    }

    fn fmt_debug(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result
    where
        Self::Value: std::fmt::Debug,
    {
        write!(f, "<future_scan>")
    }
}
impl<St, I, Scan, Map> BindSink for FutureScanNode<St, I, Scan, Map>
where
    St: 'static,
    I: 'static,
    Scan: ScanFn<St, I> + 'static,
    Map: MapFn<St> + 'static,
{
    fn notify(self: Rc<Self>, _slot: Slot, level: DirtyLevel, nc: &mut NotifyContext) {
        let mut d = self.data.borrow_mut();
        let Some(t) = d.task.as_mut() else {
            return;
        };
        if t.is_wake {
            return;
        }
        t.is_wake = true;
        self.sinks
            .borrow_mut()
            .notify(level.with_filter(Scan::FILTER), nc)
    }
}
impl<St, I, Scan, Map> BindSource for FutureScanNode<St, I, Scan, Map>
where
    St: 'static,
    I: 'static,
    Scan: ScanFn<St, I> + 'static,
    Map: MapFn<St> + 'static,
{
    fn check(self: Rc<Self>, _slot: Slot, key: BindKey, uc: &mut UpdateContext) -> bool {
        self.update(uc);
        self.sinks.borrow().is_dirty(key, uc)
    }

    fn unbind(self: Rc<Self>, _slot: Slot, key: BindKey, uc: &mut UpdateContext) {
        self.sinks.borrow_mut().unbind(key, uc);
    }

    fn rebind(self: Rc<Self>, slot: Slot, key: BindKey, sc: &mut SignalContext) {
        self.sinks.borrow_mut().rebind(self.clone(), slot, key, sc);
    }
}
