use crate::async_runtime::*;
use crate::*;
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

pub struct SubscribeAsync<F, Fut>(RefCell<SubscribeAsyncData<F, Fut>>)
where
    F: FnMut(&mut BindContext) -> Fut + 'static,
    Fut: Future<Output = ()> + 'static;

struct SubscribeAsyncData<F, Fut>
where
    F: FnMut(&mut BindContext) -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    f: F,
    bindings: Bindings,
    task: Option<Box<dyn AsyncTaskHandle>>,
    fut: Pin<Box<Option<Fut>>>,
    waker: Option<Waker>,
    is_loaded: bool,
}

impl<F, Fut> SubscribeAsync<F, Fut>
where
    F: FnMut(&mut BindContext) -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    pub fn new(f: F) -> Rc<Self> {
        let this = Rc::new(Self(RefCell::new(SubscribeAsyncData {
            f,
            bindings: Bindings::new(),
            task: None,
            fut: Box::pin(None),
            waker: None,
            is_loaded: false,
        })));
        this.0.borrow_mut().task = Some(spawn_local_async_task(&this));
        this
    }
}

impl<F, Fut> DynWeakAsyncTask for SubscribeAsync<F, Fut>
where
    F: FnMut(&mut BindContext) -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    fn poll(self: Rc<Self>, cx: &mut Context) {
        let d = &mut *self.0.borrow_mut();
        if !d.is_loaded {
            d.is_loaded = true;
            d.fut.set(None);
            let bindings = &mut d.bindings;
            let f = &mut d.f;
            let fut = BindScope::with(|scope| bindings.update(scope, &self, f));
            d.fut.set(Some(fut));
        }
        if let Some(fut) = d.fut.as_mut().as_pin_mut() {
            if let Poll::Ready(_) = fut.poll(cx) {
                d.fut.set(None);
            }
        }
        d.waker = Some(cx.waker().clone());
    }
}

impl<F, Fut> BindSink for SubscribeAsync<F, Fut>
where
    F: FnMut(&mut BindContext) -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    fn notify(self: Rc<Self>, _scope: &NotifyScope) {
        let d = &mut *self.0.borrow_mut();
        if d.is_loaded {
            d.is_loaded = false;
            if let Some(waker) = d.waker.take() {
                waker.wake();
            }
        }
    }
}
