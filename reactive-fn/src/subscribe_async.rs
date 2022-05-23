use crate::*;
use rt_local::Task;
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
    _task: Task<()>,
    fut: Pin<Box<Option<Fut>>>,
    waker: Option<Waker>,
    is_dirty: bool,
}

impl<F, Fut> SubscribeAsync<F, Fut>
where
    F: FnMut(&mut BindContext) -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    pub fn new(f: F) -> Rc<Self> {
        Rc::new_cyclic(|this| {
            Self(RefCell::new(SubscribeAsyncData {
                f,
                bindings: Bindings::new(),
                _task: spawn_local_weak_raw(this.clone()),
                fut: Box::pin(None),
                waker: None,
                is_dirty: true,
            }))
        })
    }
}

impl<F, Fut> RcFuture for SubscribeAsync<F, Fut>
where
    F: FnMut(&mut BindContext) -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    type Output = ();

    fn poll(self: Rc<Self>, cx: &mut Context) -> Poll<Self::Output> {
        let d = &mut *self.0.borrow_mut();
        if d.is_dirty {
            d.is_dirty = false;
            d.fut.set(None);
        }
        if d.fut.is_none() {
            let fut = BindScope::with(|scope| d.bindings.update(scope, &self, &mut d.f));
            d.fut.set(Some(fut));
        }
        if let Some(fut) = d.fut.as_mut().as_pin_mut() {
            if fut.poll(cx).is_ready() {
                d.fut.set(None);
            } else {
                return Poll::Pending;
            }
        }
        d.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

impl<F, Fut> BindSink for SubscribeAsync<F, Fut>
where
    F: FnMut(&mut BindContext) -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    fn notify(self: Rc<Self>, _scope: &NotifyScope) {
        let d = &mut *self.0.borrow_mut();
        if !d.is_dirty {
            d.is_dirty = true;
            if let Some(waker) = d.waker.take() {
                waker.wake();
            }
        }
    }
}
