use super::*;
use std::{
    cell::{Ref, RefCell, RefMut},
    rc::Rc,
};
pub trait Subscriber: 'static {
    type St;
    fn borrow(&self) -> Ref<Self::St>;
    fn borrow_mut(&self) -> RefMut<Self::St>;
    fn into_dyn(self) -> DynSubscriber<Self::St>;
    fn into_subscription(self) -> Subscription;
}

pub struct DynSubscriber<St>(DynSubscriberData<St>);

enum DynSubscriberData<St> {
    Subscriber(Rc<dyn InnerSubscriber<St = St>>),
    Constant(RefCell<St>),
}
impl<St: 'static> Subscriber for DynSubscriber<St> {
    type St = St;
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
            DynSubscriberData::Subscriber(s) => s.to_subscription(),
            DynSubscriberData::Constant(_) => Subscription::empty(),
        }
    }
}
impl<St: 'static> From<DynSubscriber<St>> for Subscription {
    fn from(s: DynSubscriber<St>) -> Self {
        s.into_subscription()
    }
}

pub(crate) trait InnerSubscriber: 'static {
    type St;
    fn borrow(&self) -> Ref<Self::St>;
    fn borrow_mut(&self) -> RefMut<Self::St>;
    fn to_subscription(self: Rc<Self>) -> Subscription;
}

pub(crate) fn subscriber<St: 'static>(
    rc: Rc<impl InnerSubscriber<St = St>>,
) -> impl Subscriber<St = St> {
    OuterSubscriber(rc)
}

struct OuterSubscriber<I>(Rc<I>);

impl<I: InnerSubscriber> Subscriber for OuterSubscriber<I> {
    type St = I::St;
    fn borrow(&self) -> Ref<Self::St> {
        self.0.borrow()
    }
    fn borrow_mut(&self) -> RefMut<Self::St> {
        self.0.borrow_mut()
    }
    fn into_dyn(self) -> DynSubscriber<Self::St> {
        DynSubscriber(DynSubscriberData::Subscriber(self.0))
    }
    fn into_subscription(self) -> Subscription {
        self.into_dyn().into()
    }
}

pub(crate) enum MayConstantSubscriber<S: Subscriber> {
    Subscriber(S),
    Constant(RefCell<S::St>),
}
impl<S: Subscriber> Subscriber for MayConstantSubscriber<S> {
    type St = S::St;
    fn borrow(&self) -> Ref<Self::St> {
        match self {
            Self::Subscriber(s) => s.borrow(),
            Self::Constant(o) => o.borrow(),
        }
    }
    fn borrow_mut(&self) -> RefMut<Self::St> {
        match self {
            Self::Subscriber(s) => s.borrow_mut(),
            Self::Constant(o) => o.borrow_mut(),
        }
    }
    fn into_dyn(self) -> DynSubscriber<Self::St> {
        match self {
            Self::Subscriber(s) => s.into_dyn(),
            Self::Constant(o) => DynSubscriber(DynSubscriberData::Constant(o)),
        }
    }
    fn into_subscription(self) -> Subscription {
        self.into_dyn().into_subscription()
    }
}
