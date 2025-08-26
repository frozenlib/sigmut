use std::{cell::RefCell, fmt, rc::Rc};

use crate::{
    core::{BindSink, NotifyContext, NotifyLevel, Slot, SourceBinder, UpdateContext},
    Signal, SignalContext, StateRef,
};

use super::SignalNode;

pub(crate) fn keep_node<T: ?Sized + 'static>(signal: Signal<T>) -> Signal<T> {
    Signal::from_node(KeepNode::new(signal))
}

struct KeepNode<T: ?Sized + 'static> {
    binder: RefCell<SourceBinder>,
    signal: Signal<T>,
}

impl<T: ?Sized + 'static> KeepNode<T> {
    fn new(signal: Signal<T>) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            binder: RefCell::new(SourceBinder::new(this, Slot(0))),
            signal,
        })
    }
    fn update(&self, uc: &mut UpdateContext) {
        if self.binder.borrow().is_clean() {
            return;
        }
        self.binder.borrow_mut().update(
            |sc| {
                self.signal.borrow(sc);
            },
            uc,
        );
    }
}
impl<T: ?Sized + 'static> SignalNode for KeepNode<T> {
    type Value = T;

    fn borrow<'a, 's: 'a>(
        self: Rc<Self>,
        inner: &'a Self,
        sc: &mut SignalContext<'s>,
    ) -> StateRef<'a, Self::Value> {
        self.update(sc.uc());
        inner.signal.borrow(sc)
    }

    fn fmt_debug(&self, f: &mut fmt::Formatter) -> fmt::Result
    where
        Self::Value: fmt::Debug,
    {
        write!(f, "keep({:?})", self.signal)
    }
}

impl<T: ?Sized + 'static> BindSink for KeepNode<T> {
    fn notify(self: Rc<Self>, slot: Slot, level: NotifyLevel, _nc: &mut NotifyContext) {
        self.binder.borrow_mut().on_notify(slot, level);
    }
}
