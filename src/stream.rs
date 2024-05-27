use crate::{
    core::{BindSink, DirtyOrMaybeDirty, NotifyContext, Slot, SourceBinder, Task, UpdateContext},
    SignalContext,
};
use futures::Stream;
use std::{
    cell::RefCell,
    mem::{replace, take},
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

pub fn stream_from<T: 'static>(
    f: impl FnMut(&mut SignalContext) -> T + 'static,
) -> impl Stream<Item = T> + Unpin + 'static {
    SignalStream::new(f)
}

#[derive(Default)]
enum ValueState<T> {
    #[default]
    None,
    Pending(Waker),
    Ready(T),
}

struct SignalStream<F, T>(Rc<Node<F, T>>);

struct Node<F, T>(RefCell<Data<F, T>>);

struct Data<F, T> {
    f: F,
    is_scheduled: bool,
    value: ValueState<T>,
    sb: SourceBinder,
}

impl<F, T> SignalStream<F, T>
where
    F: FnMut(&mut SignalContext) -> T + 'static,
    T: 'static,
{
    pub fn new(f: F) -> Self {
        Self(Rc::new_cyclic(|this| {
            Node(RefCell::new(Data {
                f,
                is_scheduled: false,
                value: ValueState::None,
                sb: SourceBinder::new(this, Slot(0)),
            }))
        }))
    }
}

impl<F, T> Stream for SignalStream<F, T>
where
    F: FnMut(&mut SignalContext) -> T + 'static,
    T: 'static,
{
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut d = self.0 .0.borrow_mut();
        match take(&mut d.value) {
            ValueState::None | ValueState::Pending(_) => {
                d.value = ValueState::Pending(cx.waker().clone());
                if !d.sb.is_clean() {
                    self.0.schedule(&mut d);
                }
                Poll::Pending
            }
            ValueState::Ready(value) => Poll::Ready(Some(value)),
        }
    }
}

impl<F, T> BindSink for Node<F, T>
where
    F: FnMut(&mut SignalContext) -> T + 'static,
    T: 'static,
{
    fn notify(self: Rc<Self>, slot: Slot, dirty: DirtyOrMaybeDirty, _nc: &mut NotifyContext) {
        let mut d = self.0.borrow_mut();
        if d.sb.on_notify(slot, dirty) {
            self.schedule(&mut d);
        }
    }
}

impl<F, T> Node<F, T>
where
    F: FnMut(&mut SignalContext) -> T + 'static,
    T: 'static,
{
    fn schedule(self: &Rc<Self>, d: &mut Data<F, T>) {
        if !d.is_scheduled {
            d.is_scheduled = true;
            Task::from_weak_fn(Rc::downgrade(self), Node::update).schedule();
        }
    }

    fn update(self: Rc<Self>, uc: &mut UpdateContext) {
        let d = &mut *self.0.borrow_mut();
        d.is_scheduled = false;
        if d.sb.check(uc) {
            let value = d.sb.update(|sc| (d.f)(sc), uc);
            if let ValueState::Pending(waker) = replace(&mut d.value, ValueState::Ready(value)) {
                waker.wake();
            }
        }
    }
}
