use crate::*;
use rt_local::Task;
use std::future::Future;
use std::task::Waker;
use std::{
    cell::RefCell,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

pub struct ObsFromAsync<Fut>
where
    Fut: Future + 'static,
{
    data: RefCell<ObsFromAsyncData<Fut>>,
    sinks: BindSinks,
}

struct ObsFromAsyncData<Fut>
where
    Fut: Future + 'static,
{
    task: Option<Task<()>>,
    fut: Option<Pin<Box<Fut>>>,
    waker: Option<Waker>,
    value: Poll<Fut::Output>,
}
impl<Fut> ObsFromAsyncData<Fut>
where
    Fut: Future + 'static,
{
    fn new(future: Fut) -> Self {
        Self {
            task: None,
            fut: Some(Box::pin(future)),
            waker: None,
            value: Poll::Pending,
        }
    }
    fn is_need_wake(&self) -> bool {
        self.fut.is_some() && (self.task.is_none() || self.waker.is_some())
    }
}

impl<Fut> ObsFromAsync<Fut>
where
    Fut: Future + 'static,
{
    pub fn new(future: Fut) -> Rc<Self> {
        Rc::new(Self {
            data: RefCell::new(ObsFromAsyncData::new(future)),
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
impl<Fut> Observable for Rc<ObsFromAsync<Fut>>
where
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

impl<Fut> BindSource for ObsFromAsync<Fut>
where
    Fut: Future + 'static,
{
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}

impl<Fut> RcFuture for ObsFromAsync<Fut>
where
    Fut: Future + 'static,
{
    type Output = ();
    fn poll(self: Rc<Self>, cx: &mut Context) -> Poll<()> {
        let mut d = self.data.borrow_mut();
        if !self.sinks.is_empty() {
            if let Some(fut) = d.fut.as_mut() {
                d.value = fut.as_mut().poll(cx);
                return if d.value.is_ready() {
                    d.task.take();
                    d.fut.take();
                    d.waker.take();
                    NotifyScope::with(|scope| self.sinks.notify(scope));
                    Poll::Ready(())
                } else {
                    Poll::Pending
                };
            }
        }
        d.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}
