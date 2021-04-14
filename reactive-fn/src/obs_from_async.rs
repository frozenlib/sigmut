use crate::async_runtime::*;
use crate::*;
use std::future::Future;
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
    task: Option<AsyncTaskHandle>,
    fut: Option<Pin<Box<Fut>>>,
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
            value: Poll::Pending,
        }
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
    fn update(self: &Rc<Self>) {
        let d = &mut *self.data.borrow_mut();
        if !self.sinks.is_empty() {
            if d.fut.is_some() && d.task.is_none() {
                d.task = Some(spawn_local_weak(self));
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
        cx: &mut BindContext,
    ) -> U {
        cx.bind(self.clone());
        self.update();
        f(&self.data.borrow().value, cx)
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

impl<Fut> AsyncTask for ObsFromAsync<Fut>
where
    Fut: Future + 'static,
{
    fn poll(self: Rc<Self>, cx: &mut Context) {
        let mut is_notify = false;
        let d = &mut *self.data.borrow_mut();
        if d.value.is_pending() {
            if let Some(fut) = d.fut.as_mut() {
                d.value = fut.as_mut().poll(cx);
                if d.value.is_ready() {
                    is_notify = true;
                    d.task.take();
                    d.fut.take();
                }
            }
        }
        drop(d);
        if is_notify {
            NotifyScope::with(|scope| self.sinks.notify(scope));
        }
    }
}
