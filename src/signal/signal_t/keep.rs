use std::{any::Any, cell::RefCell, fmt, rc::Rc};

use crate::{
    Signal, SignalContext, StateRef,
    core::{BindSink, DirtyLevel, NotifyContext, Slot, SourceBinder, ReactionContext},
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
    fn update(&self, rc: &mut ReactionContext) {
        if self.binder.borrow().is_clean() {
            return;
        }
        self.binder.borrow_mut().update(
            |sc| {
                self.signal.borrow(sc);
            },
            rc,
        );
    }
}
impl<T: ?Sized + 'static> SignalNode for KeepNode<T> {
    type Value = T;

    fn borrow<'a, 's: 'a>(
        &'a self,
        rc_self: Rc<dyn Any>,
        sc: &mut SignalContext<'s>,
    ) -> StateRef<'a, Self::Value> {
        rc_self.downcast::<Self>().unwrap().update(sc.rc());
        self.signal.borrow(sc)
    }

    fn fmt_debug(&self, f: &mut fmt::Formatter) -> fmt::Result
    where
        Self::Value: fmt::Debug,
    {
        write!(f, "keep({:?})", self.signal)
    }
}

impl<T: ?Sized + 'static> BindSink for KeepNode<T> {
    fn notify(self: Rc<Self>, slot: Slot, level: DirtyLevel, _nc: &mut NotifyContext) {
        self.binder.borrow_mut().on_notify(slot, level);
    }
}


