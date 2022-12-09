use crate::*;
use futures_core::Stream;
use std::{
    cell::RefCell,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

pub struct IntoStream<S>(Rc<IntoStreamData<S>>);
struct IntoStreamData<S> {
    source: ImplObs<S>,
    state: RefCell<IntoStreamState>,
}

struct IntoStreamState {
    is_ready: bool,
    bindings: Bindings,
    waker: Option<Waker>,
}

impl<S> IntoStream<S> {
    pub fn new(source: ImplObs<S>) -> Self {
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

impl<S> BindSink for IntoStreamData<S>
where
    S: Observable + 'static,
{
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
impl<S> Stream for IntoStream<S>
where
    S: Observable + 'static,
    S::Item: ToOwned,
{
    type Item = <S::Item as ToOwned>::Owned;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let this = &self.as_ref().0;
        let b = &mut *this.state.borrow_mut();
        if b.is_ready {
            b.is_ready = false;
            let bindings = &mut b.bindings;
            let value =
                BindScope::with(|scope| bindings.update(scope, this, |bc| this.source.get(bc)));
            Poll::Ready(Some(value))
        } else {
            b.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
