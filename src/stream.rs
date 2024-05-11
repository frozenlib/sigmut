use crate::{
    core::{
        BindSink, Dirty, DirtyOrMaybeDirty, NotifyContext, Slot, SourceBindings, Task,
        UpdateContext,
    },
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
    dirty: Dirty,
    is_scheduled: bool,
    value: ValueState<T>,
    sources: SourceBindings,
}

impl<F, T> SignalStream<F, T> {
    pub fn new(f: F) -> Self {
        Self(Rc::new(Node(RefCell::new(Data {
            f,
            dirty: Dirty::Dirty,
            is_scheduled: false,
            value: ValueState::None,
            sources: SourceBindings::new(),
        }))))
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
                if !d.dirty.is_clean() {
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
    fn notify(self: Rc<Self>, _slot: Slot, dirty: DirtyOrMaybeDirty, _nc: &mut NotifyContext) {
        let mut d = self.0.borrow_mut();
        if d.dirty.is_clean() {
            self.schedule(&mut d);
        }
        d.dirty |= dirty;
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
        if d.dirty.check(&mut d.sources, uc) {
            let sink = Rc::downgrade(&self);
            let value = d.sources.update(sink, Slot(0), true, |sc| (d.f)(sc), uc);
            d.dirty = Dirty::Clean;
            if let ValueState::Pending(waker) = replace(&mut d.value, ValueState::Ready(value)) {
                waker.wake();
            }
        }
    }
}
