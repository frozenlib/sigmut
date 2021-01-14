use super::*;
use std::{
    any::Any,
    cell::{Ref, RefCell, RefMut},
    rc::Rc,
};
pub trait Subscriber<O>: 'static {
    fn borrow(&self) -> Ref<O>;
    fn borrow_mut(&self) -> RefMut<O>;
    fn into_dyn(self) -> DynSubscriber<O>;
    fn into_subscription(self) -> Subscription;
}

pub struct DynSubscriber<O>(DynSubscriberData<O>);

enum DynSubscriberData<O> {
    Subscriber(Rc<dyn InnerSubscriber<O>>),
    Constant(RefCell<O>),
}
impl<O: 'static> Subscriber<O> for DynSubscriber<O> {
    fn borrow(&self) -> Ref<O> {
        match &self.0 {
            DynSubscriberData::Subscriber(s) => s.borrow(),
            DynSubscriberData::Constant(o) => o.borrow(),
        }
    }
    fn borrow_mut(&self) -> RefMut<O> {
        match &self.0 {
            DynSubscriberData::Subscriber(s) => s.borrow_mut(),
            DynSubscriberData::Constant(o) => o.borrow_mut(),
        }
    }
    fn into_dyn(self) -> DynSubscriber<O> {
        self
    }
    fn into_subscription(self) -> Subscription {
        match self.0 {
            DynSubscriberData::Subscriber(s) => Subscription(Some(s.as_rc_any())),
            DynSubscriberData::Constant(_) => Subscription::empty(),
        }
    }
}
impl<O: 'static> From<DynSubscriber<O>> for Subscription {
    fn from(s: DynSubscriber<O>) -> Self {
        s.into_subscription()
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
    fn into_dyn(self) -> DynSubscriber<O> {
        DynSubscriber(DynSubscriberData::Subscriber(self.0))
    }
    fn into_subscription(self) -> Subscription {
        self.into_dyn().into()
    }
}

pub(crate) enum MayConstantSubscriber<S, O> {
    Subscriber(S),
    Constant(RefCell<O>),
}
impl<S: Subscriber<O>, O: 'static> Subscriber<O> for MayConstantSubscriber<S, O> {
    fn borrow(&self) -> Ref<O> {
        match self {
            Self::Subscriber(s) => s.borrow(),
            Self::Constant(o) => o.borrow(),
        }
    }
    fn borrow_mut(&self) -> RefMut<O> {
        match self {
            Self::Subscriber(s) => s.borrow_mut(),
            Self::Constant(o) => o.borrow_mut(),
        }
    }
    fn into_dyn(self) -> DynSubscriber<O> {
        match self {
            Self::Subscriber(s) => s.into_dyn(),
            Self::Constant(o) => DynSubscriber(DynSubscriberData::Constant(o)),
        }
    }
    fn into_subscription(self) -> Subscription {
        self.into_dyn().into_subscription()
    }
}
