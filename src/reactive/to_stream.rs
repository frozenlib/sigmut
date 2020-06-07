use super::*;
use futures::Stream;
use std::{
    pin::Pin,
    task::{Context, Poll, Waker},
};

pub struct ToStream<T: 'static>(Rc<ToStreamData<T>>);
struct ToStreamData<T: 'static> {
    source: Re<T>,
    state: RefCell<ToStreamState>,
}

struct ToStreamState {
    is_ready: bool,
    bindings: Bindings,
    waker: Option<Waker>,
}

impl<T: 'static> ToStream<T> {
    pub fn new(source: Re<T>) -> Self {
        Self(Rc::new(ToStreamData {
            source,
            state: RefCell::new(ToStreamState {
                is_ready: true,
                bindings: Bindings::new(),
                waker: None,
            }),
        }))
    }
}

impl<T: 'static> BindSink for ToStreamData<T> {
    fn notify(self: Rc<Self>, _ctx: &NotifyContext) {
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
impl<T: 'static> Stream for ToStream<T> {
    type Item = T;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let this = &self.as_ref().0;
        let b = &mut *this.state.borrow_mut();
        if b.is_ready {
            b.is_ready = false;
            let bindings = &mut b.bindings;
            let value = BindContextScope::with(|scope| {
                bindings.update(scope, this, |ctx| this.source.get(ctx))
            });
            Poll::Ready(Some(value))
        } else {
            b.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
