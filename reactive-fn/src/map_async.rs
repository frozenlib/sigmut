use crate::*;
use rt_local::Task;
use std::future::Future;
use std::{
    cell::RefCell,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

pub struct MapAsync<F, Fut>
where
    F: Fn(&mut BindContext) -> Fut + 'static,
    Fut: Future + 'static,
{
    data: RefCell<MapAsyncData<F, Fut>>,
    sinks: BindSinks,
}

struct MapAsyncData<F, Fut>
where
    F: Fn(&mut BindContext) -> Fut + 'static,
    Fut: Future + 'static,
{
    f: F,
    bindings: Bindings,
    task: Option<Task<()>>,
    fut: Pin<Box<Option<Fut>>>,
    waker: Option<Waker>,
    value: Poll<Fut::Output>,
    is_dirty: bool,
}
impl<F, Fut> MapAsyncData<F, Fut>
where
    F: Fn(&mut BindContext) -> Fut + 'static,
    Fut: Future + 'static,
{
    fn new(f: F) -> Self {
        Self {
            f,
            bindings: Bindings::new(),
            task: None,
            waker: None,
            fut: Box::pin(None),
            value: Poll::Pending,
            is_dirty: false,
        }
    }
    fn is_need_wake(&self) -> bool {
        self.value.is_pending()
            && self.fut.is_none()
            && (self.task.is_none() || self.waker.is_some())
    }
}

impl<F, Fut> MapAsync<F, Fut>
where
    F: Fn(&mut BindContext) -> Fut + 'static,
    Fut: Future + 'static,
{
    pub fn new(f: F) -> Rc<Self> {
        Rc::new(Self {
            data: RefCell::new(MapAsyncData::new(f)),
            sinks: BindSinks::new(),
        })
    }
    fn wake(self: &Rc<Self>) {
        if !self.sinks.is_empty() && self.data.borrow().is_need_wake() {
            let mut d = self.data.borrow_mut();
            if d.task.is_none() {
                d.task = Some(spawn_local_weak(self));
            } else if let Some(waker) = d.waker.take() {
                waker.wake();
            }
        }
    }
}
impl<F, Fut> Observable for Rc<MapAsync<F, Fut>>
where
    F: Fn(&mut BindContext) -> Fut + 'static,
    Fut: Future + 'static,
{
    type Item = Poll<Fut::Output>;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        bc: &mut BindContext,
    ) -> U {
        bc.bind(self.clone());
        self.wake();
        f(&self.data.borrow().value, bc)
    }
}

impl<F, Fut> BindSource for MapAsync<F, Fut>
where
    F: Fn(&mut BindContext) -> Fut + 'static,
    Fut: Future + 'static,
{
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
    fn detach_sink(self: Rc<Self>, idx: usize) {
        self.sinks.detach(idx);
        if self.sinks.is_empty() {
            call_on_idle(&self);
        }
    }
}
impl<F, Fut> BindSink for MapAsync<F, Fut>
where
    F: Fn(&mut BindContext) -> Fut + 'static,
    Fut: Future + 'static,
{
    fn notify(self: Rc<Self>, _scope: &NotifyScope) {
        let mut d = self.data.borrow_mut();
        d.is_dirty |= d.value.is_ready();
        d.value = Poll::Pending;
        d.fut.set(None);
        if let Some(waker) = d.waker.take() {
            waker.wake();
        }
    }
}

impl<F, Fut> RcFuture for MapAsync<F, Fut>
where
    F: Fn(&mut BindContext) -> Fut + 'static,
    Fut: Future + 'static,
{
    type Output = ();

    fn poll(self: Rc<Self>, cx: &mut Context) -> Poll<()> {
        let mut d = &mut *self.data.borrow_mut();
        d.waker = Some(cx.waker().clone());
        if !self.sinks.is_empty() && d.value.is_pending() {
            if d.fut.is_none() {
                let fut = BindScope::with(|scope| d.bindings.update(scope, &self, &mut d.f));
                d.fut.set(Some(fut));
            }
            if let Some(fut) = d.fut.as_mut().as_pin_mut() {
                d.value = fut.poll(cx);
                if d.value.is_ready() || d.is_dirty {
                    d.is_dirty = false;
                    NotifyScope::with(|scope| self.sinks.notify(scope));
                }
            }
        }
        Poll::Pending
    }
}

impl<F, Fut> IdleTask for MapAsync<F, Fut>
where
    F: Fn(&mut BindContext) -> Fut + 'static,
    Fut: Future + 'static,
{
    fn call(self: Rc<Self>) {
        if self.sinks.is_empty() {
            let mut d = self.data.borrow_mut();
            d.bindings.clear();
            d.fut.set(None);
            d.value = Poll::Pending;
        }
    }
}
