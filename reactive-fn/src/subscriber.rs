use super::*;
use std::{
    any::Any,
    cell::{Ref, RefCell, RefMut},
    rc::Rc,
};
pub trait Subscriber<St>: 'static {
    fn borrow(&self) -> Ref<St>;
    fn borrow_mut(&self) -> RefMut<St>;
    fn into_dyn(self) -> DynSubscriber<St>;
    fn into_subscription(self) -> Subscription;
}

pub struct DynSubscriber<St>(DynSubscriberData<St>);

enum DynSubscriberData<St> {
    Subscriber(Rc<dyn InnerSubscriber<St>>),
    Constant(RefCell<St>),
}
impl<St: 'static> Subscriber<St> for DynSubscriber<St> {
    fn borrow(&self) -> Ref<St> {
        match &self.0 {
            DynSubscriberData::Subscriber(s) => s.borrow(),
            DynSubscriberData::Constant(o) => o.borrow(),
        }
    }
    fn borrow_mut(&self) -> RefMut<St> {
        match &self.0 {
            DynSubscriberData::Subscriber(s) => s.borrow_mut(),
            DynSubscriberData::Constant(o) => o.borrow_mut(),
        }
    }
    fn into_dyn(self) -> DynSubscriber<St> {
        self
    }
    fn into_subscription(self) -> Subscription {
        match self.0 {
            DynSubscriberData::Subscriber(s) => Subscription(Some(s.as_rc_any())),
            DynSubscriberData::Constant(_) => Subscription::empty(),
        }
    }
}
impl<St: 'static> From<DynSubscriber<St>> for Subscription {
    fn from(s: DynSubscriber<St>) -> Self {
        s.into_subscription()
    }
}

pub(crate) trait InnerSubscriber<St>: 'static {
    fn borrow(&self) -> Ref<St>;
    fn borrow_mut(&self) -> RefMut<St>;
    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any>;
}

pub(crate) fn subscriber<St: 'static>(rc: Rc<impl InnerSubscriber<St>>) -> impl Subscriber<St> {
    OuterSubscriber(rc)
}

struct OuterSubscriber<I>(Rc<I>);

impl<I: InnerSubscriber<St>, St: 'static> Subscriber<St> for OuterSubscriber<I> {
    fn borrow(&self) -> Ref<St> {
        self.0.borrow()
    }
    fn borrow_mut(&self) -> RefMut<St> {
        self.0.borrow_mut()
    }
    fn into_dyn(self) -> DynSubscriber<St> {
        DynSubscriber(DynSubscriberData::Subscriber(self.0))
    }
    fn into_subscription(self) -> Subscription {
        self.into_dyn().into()
    }
}

pub(crate) enum MayConstantSubscriber<S, St> {
    Subscriber(S),
    Constant(RefCell<St>),
}
impl<S: Subscriber<St>, St: 'static> Subscriber<St> for MayConstantSubscriber<S, St> {
    fn borrow(&self) -> Ref<St> {
        match self {
            Self::Subscriber(s) => s.borrow(),
            Self::Constant(o) => o.borrow(),
        }
    }
    fn borrow_mut(&self) -> RefMut<St> {
        match self {
            Self::Subscriber(s) => s.borrow_mut(),
            Self::Constant(o) => o.borrow_mut(),
        }
    }
    fn into_dyn(self) -> DynSubscriber<St> {
        match self {
            Self::Subscriber(s) => s.into_dyn(),
            Self::Constant(o) => DynSubscriber(DynSubscriberData::Constant(o)),
        }
    }
    fn into_subscription(self) -> Subscription {
        self.into_dyn().into_subscription()
    }
}
