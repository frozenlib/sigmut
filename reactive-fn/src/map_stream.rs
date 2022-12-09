use crate::*;
use futures_core::Stream;
use rt_local_core::Task;
use std::{
    cell::RefCell,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

pub struct MapStream<F, St>
where
    F: Fn(&mut ObsContext) -> St + 'static,
    St: Stream + 'static,
{
    initial_value: St::Item,
    data: RefCell<MapStreamData<F, St>>,
    sinks: BindSinks,
}

struct MapStreamData<F, St>
where
    F: Fn(&mut ObsContext) -> St + 'static,
    St: Stream + 'static,
{
    f: F,
    bindings: Bindings,
    task: Option<Task<()>>,
    stream: Pin<Box<Option<St>>>,
    waker: Option<Waker>,
    value: Option<St::Item>,
    is_dirty: bool,
}
impl<F, St> MapStreamData<F, St>
where
    F: Fn(&mut ObsContext) -> St + 'static,
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
            is_dirty: false,
        }
    }
    fn is_need_wake(&self) -> bool {
        self.value.is_none()
            && self.stream.is_none()
            && (self.task.is_none() || self.waker.is_some())
    }
}

impl<F, St> MapStream<F, St>
where
    F: Fn(&mut ObsContext) -> St + 'static,
    St: Stream + 'static,
{
    pub fn new(initial_value: St::Item, f: F) -> Rc<Self> {
        Rc::new(Self {
            initial_value,
            data: RefCell::new(MapStreamData::new(f)),
            sinks: BindSinks::new(),
        })
    }
    fn wake(self: &Rc<Self>) {
        if !self.sinks.is_empty() && self.data.borrow().is_need_wake() {
            let mut d = self.data.borrow_mut();
            if d.task.is_none() {
                d.task = Some(spawn_local_weak_from(self));
            } else if let Some(waker) = d.waker.take() {
                waker.wake();
            }
        }
    }
    fn on_idle(self: Rc<Self>) {
        if self.sinks.is_empty() {
            let mut d = self.data.borrow_mut();
            d.bindings.clear();
            d.stream.set(None);
            d.value = None;
        }
    }
}
impl<F, St> Observable for Rc<MapStream<F, St>>
where
    F: Fn(&mut ObsContext) -> St + 'static,
    St: Stream + 'static,
{
    type Item = St::Item;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, bc: &mut ObsContext) -> U {
        bc.bind(self.clone());
        self.wake();
        f(
            self.data
                .borrow()
                .value
                .as_ref()
                .unwrap_or(&self.initial_value),
            bc,
        )
    }
}

impl<F, St> BindSource for MapStream<F, St>
where
    F: Fn(&mut ObsContext) -> St + 'static,
    St: Stream + 'static,
{
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
    fn on_sinks_empty(self: Rc<Self>) {
        Action::new(self, Self::on_idle).schedule_idle();
    }
}
impl<F, St> BindSink for MapStream<F, St>
where
    F: Fn(&mut ObsContext) -> St + 'static,
    St: Stream + 'static,
{
    fn notify(self: Rc<Self>, _scope: &NotifyScope) {
        let mut d = self.data.borrow_mut();
        d.is_dirty |= d.value.is_some();
        d.value = None;
        d.stream.set(None);
        if let Some(waker) = d.waker.take() {
            waker.wake();
        }
    }
}

impl<F, St> RcFuture for MapStream<F, St>
where
    F: Fn(&mut ObsContext) -> St + 'static,
    St: Stream + 'static,
{
    type Output = ();

    fn poll(self: Rc<Self>, cx: &mut Context) -> Poll<()> {
        let mut d = &mut *self.data.borrow_mut();
        d.waker = Some(cx.waker().clone());
        if !self.sinks.is_empty() && d.value.is_none() {
            if d.stream.is_none() {
                let stream = BindScope::with(|scope| d.bindings.update(scope, &self, &mut d.f));
                d.stream.set(Some(stream));
            }
            if let Some(stream) = d.stream.as_mut().as_pin_mut() {
                let value = stream.poll_next(cx);
                let is_notify = match value {
                    Poll::Ready(Some(value)) => {
                        d.value = Some(value);
                        true
                    }
                    Poll::Ready(None) => {
                        d.stream.set(None);
                        false
                    }
                    Poll::Pending => false,
                };
                if is_notify || d.is_dirty {
                    d.is_dirty = false;
                    self.sinks.notify_with_new_scope();
                }
            }
        }
        Poll::Pending
    }
}
