use super::from_async::FromAsync;
use crate::{
    core::{
        schedule_update, BindSink, CallUpdate, Computed, ObsContext, SourceBindings, UpdateContext,
    },
    AsyncObsContext,
};
use derive_ex::derive_ex;
use futures::Future;
use std::{
    any::Any,
    cell::RefCell,
    rc::{Rc, Weak},
};

const SLOT: usize = 0;

#[must_use]
#[derive_ex(Default)]
#[default(Self::empty())]
pub struct Subscription(SubscriptionInner);

impl Subscription {
    pub fn new(mut f: impl FnMut(&mut ObsContext) + 'static) -> Self {
        Self::new_while(move |oc| {
            f(oc);
            true
        })
    }
    pub fn new_while(f: impl FnMut(&mut ObsContext) -> bool + 'static) -> Self {
        let rc = Rc::new(RawSubscription(RefCell::new(Data {
            f,
            is_scheduled_update: false,
            computed: Computed::None,
            bindings: SourceBindings::new(),
        })));
        let node = Rc::downgrade(&rc);
        schedule_update(node, SLOT);
        Self::from_rc(rc)
    }
    pub fn new_future<Fut>(f: impl FnMut(&mut ObsContext) -> Fut + 'static) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        let mut f = f;
        Self::new_async(move |mut oc| oc.get(&mut f))
    }
    pub fn new_async<Fut>(f: impl FnMut(AsyncObsContext) -> Fut + 'static) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        Self::from_rc(FromAsync::new(f, true))
    }

    pub fn empty() -> Self {
        Self(SubscriptionInner::None)
    }
    pub fn from_rc(rc: Rc<dyn Any>) -> Self {
        Self(SubscriptionInner::RcAny(rc))
    }
    pub fn from_rc_slot(rc: Rc<dyn Unsubscribe>, slot: usize) -> Self {
        Self(SubscriptionInner::RcUnsubscribe { rc, slot })
    }
    pub fn from_weak_slot(weak: Weak<dyn Unsubscribe>, slot: usize) -> Self {
        Self(SubscriptionInner::WeakUnsubscribe { weak, slot })
    }
    pub fn unsubscribe(&mut self) {
        *self = Self::empty();
    }
}
impl Drop for Subscription {
    fn drop(&mut self) {
        match &mut self.0 {
            SubscriptionInner::None => {}
            SubscriptionInner::RcAny(_) => {}
            SubscriptionInner::RcUnsubscribe { rc, slot } => {
                rc.clone().unsubscribe(*slot);
            }
            SubscriptionInner::WeakUnsubscribe { weak, slot } => {
                if let Some(rc) = weak.upgrade() {
                    rc.unsubscribe(*slot);
                }
            }
        }
    }
}

enum SubscriptionInner {
    None,
    RcAny(Rc<dyn Any>),
    RcUnsubscribe {
        rc: Rc<dyn Unsubscribe>,
        slot: usize,
    },
    WeakUnsubscribe {
        weak: Weak<dyn Unsubscribe>,
        slot: usize,
    },
}
pub trait Unsubscribe {
    fn unsubscribe(self: Rc<Self>, slot: usize);
}

struct Data<F> {
    f: F,
    is_scheduled_update: bool,
    computed: Computed,
    bindings: SourceBindings,
}
struct RawSubscription<F>(RefCell<Data<F>>);

impl<F: FnMut(&mut ObsContext) -> bool + 'static> BindSink for RawSubscription<F> {
    fn notify(self: Rc<Self>, _slot: usize, is_modified: bool, uc: &mut UpdateContext) {
        let mut is_schedule = false;
        if let Ok(mut d) = self.0.try_borrow_mut() {
            if d.computed.modify(is_modified) && !d.is_scheduled_update {
                d.is_scheduled_update = true;
                is_schedule = true;
            }
        }
        if is_schedule {
            uc.schedule_update(self, SLOT);
        }
    }
}

impl<F: FnMut(&mut ObsContext) -> bool + 'static> CallUpdate for RawSubscription<F> {
    fn call_update(self: Rc<Self>, _slot: usize, uc: &mut UpdateContext) {
        let mut d = self.0.borrow_mut();
        let d = &mut *d;
        d.is_scheduled_update = false;
        if d.computed == Computed::MayBeOutdated {
            if d.bindings.flush(uc) {
                d.computed = Computed::Outdated;
            } else {
                d.computed = Computed::UpToDate;
            }
        }
        if d.computed != Computed::UpToDate {
            d.computed = Computed::UpToDate;
            let node = Rc::downgrade(&self);
            if !d.bindings.compute(node, SLOT, |oc| (d.f)(oc.reset()), uc) {
                d.bindings.clear(uc);
            }
        }
    }
}
