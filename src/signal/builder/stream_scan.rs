use std::{
    any::Any,
    cell::{Ref, RefCell},
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

use futures::Stream;

use crate::{
    Signal, SignalContext, StateRef,
    core::{
        BindKey, BindSink, BindSource, DirtyLevel, NotifyContext, SinkBindings, Slot,
        UpdateContext, waker_from_sink,
    },
};

use super::{Build, MapFn, MapFnNone, MapFnRaw, SignalNode};

pub fn stream_scan_builder<St, I>(
    initial_state: St,
    stream: impl Stream<Item = I> + 'static,
    scan: impl FnMut(&mut St, Option<I>) -> bool + 'static,
) -> impl Build<State = St>
where
    St: 'static,
    I: 'static,
{
    StreamScanBuilder {
        initial_state,
        stream: Box::pin(stream),
        scan,
        map: MapFnNone,
    }
}

struct StreamScanBuilder<St, I, Scan, Map> {
    initial_state: St,
    stream: Pin<Box<dyn Stream<Item = I>>>,
    scan: Scan,
    map: Map,
}
impl<St, I, Scan, Map> Build for StreamScanBuilder<St, I, Scan, Map>
where
    St: 'static,
    I: 'static,
    Scan: FnMut(&mut St, Option<I>) -> bool + 'static,
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
        StreamScanBuilder {
            initial_state: self.initial_state,
            stream: self.stream,
            scan: self.scan,
            map: MapFnRaw { m: self.map, f },
        }
    }

    fn build(self) -> Signal<Self::State> {
        Signal::from_node(StreamScanNode::new(
            self.initial_state,
            self.stream,
            self.scan,
            self.map,
        ))
    }
}

struct StreamScanNodeTask<I, Scan> {
    stream: Pin<Box<dyn Stream<Item = I>>>,
    is_wake: bool,
    scan: Scan,
    waker: Waker,
}

struct StreamScanNodeData<St, I, Scan> {
    state: St,
    task: Option<StreamScanNodeTask<I, Scan>>,
}

impl<St, I, Scan> StreamScanNodeData<St, I, Scan> {
    fn is_wake(&self) -> bool {
        if let Some(task) = &self.task {
            task.is_wake
        } else {
            false
        }
    }
}

struct StreamScanNode<St, I, Scan, Map> {
    sinks: RefCell<SinkBindings>,
    data: RefCell<StreamScanNodeData<St, I, Scan>>,
    map: Map,
}

impl<St, I, Scan, Map> StreamScanNode<St, I, Scan, Map>
where
    St: 'static,
    I: 'static,
    Scan: FnMut(&mut St, Option<I>) -> bool + 'static,
    Map: MapFn<St> + 'static,
{
    fn new(
        initial_state: St,
        stream: Pin<Box<dyn Stream<Item = I>>>,
        scan: Scan,
        map: Map,
    ) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            sinks: RefCell::new(SinkBindings::new()),
            data: RefCell::new(StreamScanNodeData {
                state: initial_state,
                task: Some(StreamScanNodeTask {
                    stream,
                    is_wake: true,
                    scan,
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
        let mut is_dirty = false;
        if let Poll::Ready(item) = t
            .stream
            .as_mut()
            .poll_next(&mut Context::from_waker(&t.waker))
        {
            let is_end = item.is_none();
            is_dirty = (t.scan)(&mut d.state, item);
            if is_end {
                d.task.take();
            }
        }
        if let Some(t) = d.task.as_mut() {
            t.is_wake = false;
        }
        self.sinks.borrow_mut().update(is_dirty, uc);
    }
}
impl<St, I, Scan, Map> SignalNode for StreamScanNode<St, I, Scan, Map>
where
    St: 'static,
    I: 'static,
    Scan: FnMut(&mut St, Option<I>) -> bool + 'static,
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
        write!(f, "<stream_scan>")
    }
}
impl<St, I, Scan, Map> BindSink for StreamScanNode<St, I, Scan, Map>
where
    St: 'static,
    I: 'static,
    Scan: FnMut(&mut St, Option<I>) -> bool + 'static,
    Map: MapFn<St> + 'static,
{
    fn notify(self: Rc<Self>, _slot: Slot, _level: DirtyLevel, nc: &mut NotifyContext) {
        let mut d = self.data.borrow_mut();
        let Some(t) = d.task.as_mut() else {
            return;
        };
        if t.is_wake {
            return;
        }
        t.is_wake = true;
        self.sinks.borrow_mut().notify(DirtyLevel::MaybeDirty, nc)
    }
}
impl<St, I, Scan, Map> BindSource for StreamScanNode<St, I, Scan, Map>
where
    St: 'static,
    I: 'static,
    Scan: FnMut(&mut St, Option<I>) -> bool + 'static,
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
