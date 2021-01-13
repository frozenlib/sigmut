use crate::*;
use std::{
    any::Any,
    cell::{Ref, RefMut},
    rc::Rc,
};
pub trait Subscriber<O> {
    fn borrow(&self) -> Ref<O>;
    fn borrow_mut(&self) -> RefMut<O>;
    fn as_dyn(&self) -> DynSubscriber<O>;
    fn as_subscription(&self) -> Subscription;
}

pub struct DynSubscriber<O>(Rc<dyn InnerSubscriber<O>>);

impl<O: 'static> Subscriber<O> for DynSubscriber<O> {
    fn borrow(&self) -> Ref<O> {
        self.0.borrow()
    }
    fn borrow_mut(&self) -> RefMut<O> {
        self.0.borrow_mut()
    }
    fn as_dyn(&self) -> DynSubscriber<O> {
        self.clone()
    }
    fn as_subscription(&self) -> Subscription {
        self.clone().into()
    }
}
impl<O: 'static> Clone for DynSubscriber<O> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<O: 'static> From<DynSubscriber<O>> for Subscription {
    fn from(s: DynSubscriber<O>) -> Self {
        Self(Some(s.0.as_rc_any()))
    }
}

pub(crate) trait InnerSubscriber<O>: 'static {
    fn borrow(&self) -> Ref<O>;
    fn borrow_mut(&self) -> RefMut<O>;
    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any>;
}

pub(crate) fn subscriber<O: 'static>(rc: Rc<impl InnerSubscriber<O>>) -> impl Subscriber<O> {
    OuterSubscriber(rc)
}

struct OuterSubscriber<I>(Rc<I>);

impl<I: InnerSubscriber<O>, O: 'static> Subscriber<O> for OuterSubscriber<I> {
    fn borrow(&self) -> Ref<O> {
        self.0.borrow()
    }
    fn borrow_mut(&self) -> RefMut<O> {
        self.0.borrow_mut()
    }
    fn as_dyn(&self) -> DynSubscriber<O> {
        DynSubscriber(self.0.clone())
    }
    fn as_subscription(&self) -> Subscription {
        self.as_dyn().into()
    }
}
