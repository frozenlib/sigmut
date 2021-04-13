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
    fn update(self: &Rc<Self>, load: bool, scope: &BindScope) {
        let d = &mut *self.data.borrow_mut();
        if !d.is_loaded {
            d.value = Poll::Pending;
            d.fut.set(None);
            if self.sinks.is_empty() {
                d.task.take();
            }
        }
        if load && !self.sinks.is_empty() {
            if !d.is_loaded {
                d.is_loaded = true;
                d.fut.set(Some(d.bindings.update(scope, &self, &mut d.f)));
                if d.task.is_none() {
                    let task = WeakAsyncTask::from_rc(self.clone());
                    d.task = Some(with_async_runtime(|rt| rt.spawn_local(task)));
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
        self.update(true, cx.scope());
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
        if mem::replace(&mut self.data.borrow_mut().is_loaded, false) {
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
        self.update(false, scope);
    }
}

impl<F, Fut> DynWeakAsyncTask for MapAsync<F, Fut>
where
    F: Fn(&mut BindContext) -> Fut + 'static,
    Fut: Future + 'static,
{
    fn poll(&self, cx: &mut Context) {
        let d = &mut *self.data.borrow_mut();
        if let Some(fut) = d.fut.as_mut().as_pin_mut() {
            d.value = fut.poll(cx);
            if d.value.is_ready() {
                d.fut.set(None);
            }
        }
        d.waker = Some(cx.waker().clone());
    }
}
