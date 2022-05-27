use crate::*;
use futures_core::Stream;
use rt_local_core::Task;
use std::{
    cell::RefCell,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
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
    task: Option<Task<()>>,
    stream: Option<Pin<Box<St>>>,
    waker: Option<Waker>,
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
            waker: None,
            value: initial_value,
        }
    }
    fn is_need_wake(&self) -> bool {
        self.stream.is_some() && (self.task.is_none() || self.waker.is_some())
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
}
impl<St> Observable for Rc<ObsFromStream<St>>
where
    St: Stream + 'static,
{
    type Item = St::Item;

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

impl<St> BindSource for ObsFromStream<St>
where
    St: Stream + 'static,
{
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}

impl<St> RcFuture for ObsFromStream<St>
where
    St: Stream + 'static,
{
    type Output = ();

    fn poll(self: Rc<Self>, cx: &mut Context) -> Poll<()> {
        let mut d = self.data.borrow_mut();
        if !self.sinks.is_empty() {
            if let Some(stream) = d.stream.as_mut() {
                match stream.as_mut().poll_next(cx) {
                    Poll::Ready(Some(value)) => {
                        d.value = value;
                        self.sinks.notify_with_new_scope();
                    }
                    Poll::Ready(None) => {
                        d.task.take();
                        d.stream.take();
                        d.waker.take();
                        return Poll::Ready(());
                    }
                    Poll::Pending => return Poll::Pending,
                }
            }
        }
        d.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}
