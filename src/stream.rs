use crate::{
    SignalContext,
    core::{BindSink, DirtyLevel, NotifyContext, Reaction, ReactionContext, Slot, SourceBinder},
};
use futures::Stream;
use std::{
    cell::RefCell,
    mem::{replace, take},
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

/// Create a `Stream` from a signal function.
pub fn stream_from<T: 'static>(
    f: impl FnMut(&mut SignalContext<'_, '_>) -> T + 'static,
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
    F: FnMut(&mut SignalContext<'_, '_>) -> T + 'static,
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
    F: FnMut(&mut SignalContext<'_, '_>) -> T + 'static,
    T: 'static,
{
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let waker = cx.waker().clone();
        let mut d = self.0.0.borrow_mut();
        let previous = take(&mut d.value);
        if matches!(previous, ValueState::Ready(_)) {
            drop(d);
            if let ValueState::Ready(value) = previous {
                return Poll::Ready(Some(value));
            }
            unreachable!();
        }
        d.value = ValueState::Pending(waker);
        let schedule = !d.sb.is_clean() && !replace(&mut d.is_scheduled, true);
        drop(d);
        drop(previous);
        if schedule {
            self.0.schedule();
        }
        Poll::Pending
    }
}

impl<F, T> BindSink for Node<F, T>
where
    F: FnMut(&mut SignalContext<'_, '_>) -> T + 'static,
    T: 'static,
{
    fn notify(self: Rc<Self>, slot: Slot, level: DirtyLevel, _nc: &mut NotifyContext) {
        let schedule = {
            let mut d = self.0.borrow_mut();
            d.sb.on_notify(slot, level) && !replace(&mut d.is_scheduled, true)
        };
        if schedule {
            self.schedule();
        }
    }
}

impl<F, T> Node<F, T>
where
    F: FnMut(&mut SignalContext<'_, '_>) -> T + 'static,
    T: 'static,
{
    fn schedule(self: &Rc<Self>) {
        Reaction::from_weak_fn(Rc::downgrade(self), Node::update).schedule();
    }

    fn update(self: Rc<Self>, rc: &mut ReactionContext<'_, '_>) {
        let previous = {
            let mut d = self.0.borrow_mut();
            let d = &mut *d;
            d.is_scheduled = false;
            if d.sb.check(rc) {
                let value = d.sb.update(|sc| (d.f)(sc), rc);
                Some(replace(&mut d.value, ValueState::Ready(value)))
            } else {
                None
            }
        };
        if let Some(ValueState::Pending(waker)) = previous {
            waker.wake();
        }
    }
}
