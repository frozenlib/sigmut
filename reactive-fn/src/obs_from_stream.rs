use crate::async_runtime::*;
use crate::*;
use futures::Stream;
use std::{
    cell::RefCell,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

pub struct ObsFromStream<St>
where
    St: Stream + 'static,
{
    data: RefCell<ObsFromStreamData<St>>,
    sinks: BindSinks,
}

struct ObsFromStreamData<St>
where
    St: Stream + 'static,
{
    task: Option<Box<dyn AsyncTaskHandle>>,
    stream: Option<Pin<Box<St>>>,
    value: St::Item,
}
impl<St> ObsFromStreamData<St>
where
    St: Stream + 'static,
{
    fn new(initial_value: St::Item, stream: St) -> Self {
        Self {
            task: None,
            stream: Some(Box::pin(stream)),
            value: initial_value,
        }
    }
}

impl<St> ObsFromStream<St>
where
    St: Stream + 'static,
{
    pub fn new(initial_value: St::Item, stream: St) -> Rc<Self> {
        Rc::new(Self {
            data: RefCell::new(ObsFromStreamData::new(initial_value, stream)),
            sinks: BindSinks::new(),
        })
    }
    fn update(self: &Rc<Self>) {
        let d = &mut *self.data.borrow_mut();
        if !self.sinks.is_empty() {
            if d.stream.is_some() && d.task.is_none() {
                d.task = Some(spawn_local_weak(self));
            }
        }
    }
}
impl<St> Observable for Rc<ObsFromStream<St>>
where
    St: Stream + 'static,
{
    type Item = St::Item;

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

impl<St> BindSource for ObsFromStream<St>
where
    St: Stream + 'static,
{
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}

impl<St> AsyncTask for ObsFromStream<St>
where
    St: Stream + 'static,
{
    fn poll(self: Rc<Self>, cx: &mut Context) {
        let mut is_notify = false;
        let d = &mut *self.data.borrow_mut();
        if let Some(stream) = d.stream.as_mut() {
            loop {
                match stream.as_mut().poll_next(cx) {
                    Poll::Ready(Some(value)) => {
                        d.value = value;
                        is_notify = true;
                    }
                    Poll::Ready(None) => {
                        d.stream.take();
                        d.task.take();
                        break;
                    }
                    Poll::Pending => {
                        break;
                    }
                }
            }
        }
        drop(d);
        if is_notify {
            NotifyScope::with(|scope| self.sinks.notify(scope));
        }
    }
}
