use crate::async_runtime::*;
use crate::*;
use futures::Stream;
use std::{
    cell::RefCell,
    mem,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

pub struct MapStream<F, St>
where
    F: Fn(&mut BindContext) -> (St::Item, St) + 'static,
    St: Stream + 'static,
{
    data: RefCell<MapStreamData<F, St>>,
    sinks: BindSinks,
}

struct MapStreamData<F, St>
where
    F: Fn(&mut BindContext) -> (St::Item, St) + 'static,
    St: Stream + 'static,
{
    f: F,
    bindings: Bindings,
    task: Option<AsyncTaskHandle>,
    stream: Pin<Box<Option<St>>>,
    waker: Option<Waker>,
    value: Option<St::Item>,
    is_loaded: bool,
}
impl<F, St> MapStreamData<F, St>
where
    F: Fn(&mut BindContext) -> (St::Item, St) + 'static,
    St: Stream + 'static,
{
    fn new(f: F) -> Self {
        Self {
            f,
            bindings: Bindings::new(),
            task: None,
            waker: None,
            stream: Box::pin(None),
            value: None,
            is_loaded: false,
        }
    }
}

impl<F, St> MapStream<F, St>
where
    F: Fn(&mut BindContext) -> (St::Item, St) + 'static,
    St: Stream + 'static,
{
    pub fn new(f: F) -> Rc<Self> {
        Rc::new(Self {
            data: RefCell::new(MapStreamData::new(f)),
            sinks: BindSinks::new(),
        })
    }
    fn update(self: &Rc<Self>, scope: &BindScope) {
        let d = &mut *self.data.borrow_mut();
        if !d.is_loaded {
            d.value.take();
            d.stream.set(None);
            if self.sinks.is_empty() {
                d.task.take();
                d.waker.take();
            }
        }
        if !d.is_loaded {
            d.is_loaded = true;
            let (value, stream) = d.bindings.update(scope, &self, &mut d.f);
            d.stream.set(Some(stream));
            d.value = Some(value);
            if !self.sinks.is_empty() {
                if d.task.is_none() {
                    d.task = Some(spawn_local_weak(self));
                } else if let Some(waker) = d.waker.take() {
                    waker.wake();
                }
            }
        }
    }
}
impl<F, St> Observable for Rc<MapStream<F, St>>
where
    F: Fn(&mut BindContext) -> (St::Item, St) + 'static,
    St: Stream + 'static,
{
    type Item = St::Item;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        cx.bind(self.clone());
        self.update(cx.scope());
        f(&self.data.borrow().value.as_ref().unwrap(), cx)
    }
}

impl<F, St> BindSource for MapStream<F, St>
where
    F: Fn(&mut BindContext) -> (St::Item, St) + 'static,
    St: Stream + 'static,
{
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
    fn detach_sink(&self, idx: usize) {
        self.sinks.detach(idx);
        if self.sinks.is_empty() {
            let d = &mut *self.data.borrow_mut();
            d.bindings.clear();
            if d.is_loaded {
                d.is_loaded = false;
                // Runtime::spawn_bind(self);
            }
        }
    }
}
impl<F, St> BindSink for MapStream<F, St>
where
    F: Fn(&mut BindContext) -> (St::Item, St) + 'static,
    St: Stream + 'static,
{
    fn notify(self: Rc<Self>, scope: &NotifyScope) {
        let mut d = self.data.borrow_mut();
        if mem::replace(&mut d.is_loaded, false) {
            self.sinks.notify(scope);
            drop(d);
            scope.defer_bind(self);
        }
    }
}
impl<F, St> BindTask for MapStream<F, St>
where
    F: Fn(&mut BindContext) -> (St::Item, St) + 'static,
    St: Stream + 'static,
{
    fn run(self: Rc<Self>, scope: &BindScope) {
        self.update(scope);
    }
}

impl<F, St> AsyncTask for MapStream<F, St>
where
    F: Fn(&mut BindContext) -> (St::Item, St) + 'static,
    St: Stream + 'static,
{
    fn poll(self: Rc<Self>, cx: &mut Context) {
        let mut is_notify = false;
        let d = &mut *self.data.borrow_mut();
        if let Some(mut s) = d.stream.as_mut().as_pin_mut() {
            loop {
                match s.as_mut().poll_next(cx) {
                    Poll::Ready(value) => {
                        if let Some(value) = value {
                            is_notify = true;
                            d.value = Some(value);
                        } else {
                            d.stream.set(None);
                            break;
                        }
                    }
                    Poll::Pending => {
                        break;
                    }
                }
            }
        }
        d.waker = Some(cx.waker().clone());
        drop(d);
        if is_notify {
            NotifyScope::with(|scope| self.sinks.notify(scope));
        }
    }
}