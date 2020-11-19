use super::*;
use futures::Stream;
use std::{
    pin::Pin,
    task::{Context, Poll, Waker},
};

pub struct IntoStream<S>(Rc<IntoStreamData<S>>);
struct IntoStreamData<S> {
    source: S,
    state: RefCell<IntoStreamState>,
}

struct IntoStreamState {
    is_ready: bool,
    bindings: Bindings,
    waker: Option<Waker>,
}

impl<S> IntoStream<S> {
    pub fn new(source: S) -> Self {
        Self(Rc::new(IntoStreamData {
            source,
            state: RefCell::new(IntoStreamState {
                is_ready: true,
                bindings: Bindings::new(),
                waker: None,
            }),
        }))
    }
}

impl<S: Reactive> BindSink for IntoStreamData<S> {
    fn notify(self: Rc<Self>, _scope: &NotifyScope) {
        let waker = {
            let mut b = self.state.borrow_mut();
            b.is_ready = true;
            b.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}
impl<S: Reactive> Stream for IntoStream<S> {
    type Item = S::Item;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let this = &self.as_ref().0;
        let b = &mut *this.state.borrow_mut();
        if b.is_ready {
            b.is_ready = false;
            let bindings = &mut b.bindings;
            let value =
                BindScope::with(|scope| bindings.update(scope, this, |cx| this.source.get(cx)));
            Poll::Ready(Some(value))
        } else {
            b.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
