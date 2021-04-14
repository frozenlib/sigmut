use crate::async_runtime::*;
use crate::*;
use std::future::Future;
use std::{
    cell::RefCell,
    mem,
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
    task: Option<Box<dyn AsyncTaskHandle>>,
    fut: Pin<Box<Option<Fut>>>,
    waker: Option<Waker>,
    value: Poll<Fut::Output>,
    is_loaded: bool,
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
            is_loaded: false,
        }
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
    fn update(self: &Rc<Self>, scope: &BindScope) {
        let d = &mut *self.data.borrow_mut();
        if !d.is_loaded {
            d.value = Poll::Pending;
            d.fut.set(None);
            if self.sinks.is_empty() {
                d.task.take();
                d.waker.take();
            }
        }
        if !self.sinks.is_empty() {
            if !d.is_loaded {
                d.is_loaded = true;
                d.fut.set(Some(d.bindings.update(scope, &self, &mut d.f)));
                if d.task.is_none() {
                    d.task = Some(spawn_local_weak(self));
                } else if let Some(waker) = d.waker.take() {
                    waker.wake();
                }
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
        cx: &mut BindContext,
    ) -> U {
        cx.bind(self.clone());
        self.update(cx.scope());
        f(&self.data.borrow().value, cx)
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
impl<F, Fut> BindSink for MapAsync<F, Fut>
where
    F: Fn(&mut BindContext) -> Fut + 'static,
    Fut: Future + 'static,
{
    fn notify(self: Rc<Self>, scope: &NotifyScope) {
        let mut d = self.data.borrow_mut();
        if mem::replace(&mut d.is_loaded, false) {
            if d.value.is_ready() {
                self.sinks.notify(scope);
            }
            drop(d);
            scope.defer_bind(self);
        }
    }
}
impl<F, Fut> BindTask for MapAsync<F, Fut>
where
    F: Fn(&mut BindContext) -> Fut + 'static,
    Fut: Future + 'static,
{
    fn run(self: Rc<Self>, scope: &BindScope) {
        self.update(scope);
    }
}

impl<F, Fut> AsyncTask for MapAsync<F, Fut>
where
    F: Fn(&mut BindContext) -> Fut + 'static,
    Fut: Future + 'static,
{
    fn poll(self: Rc<Self>, cx: &mut Context) {
        let mut is_notify = false;
        let d = &mut *self.data.borrow_mut();
        if let Some(fut) = d.fut.as_mut().as_pin_mut() {
            d.value = fut.poll(cx);
            if d.value.is_ready() {
                d.fut.set(None);
                is_notify = true;
            }
        }
        d.waker = Some(cx.waker().clone());
        drop(d);
        if is_notify {
            NotifyScope::with(|scope| self.sinks.notify(scope));
        }
    }
}
