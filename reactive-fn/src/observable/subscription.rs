use super::from_async::FromAsync;
use crate::{
    core::{
        schedule_update_lazy, BindSink, CallUpdate, Computed, ObsContext, SourceBindings,
        UpdateContext,
    },
    AsyncObsContext,
};
use derive_ex::derive_ex;
use futures::Future;
use std::{any::Any, cell::RefCell, rc::Rc};

const SLOT: usize = 0;

#[derive_ex(Default)]
#[default(Self::empty())]
pub struct Subscription(Option<Rc<dyn Any>>);

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
        schedule_update_lazy(node, SLOT);
        Self(Some(rc))
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
        Subscription(Some(FromAsync::new(f, true)))
    }

    pub fn empty() -> Self {
        Self(None)
    }
    pub fn from_rc(rc: Rc<dyn Any>) -> Self {
        Self(Some(rc))
    }
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
            if !d.bindings.compute(node, SLOT, |cc| (d.f)(cc.oc()), uc) {
                d.bindings.clear(uc);
            }
        }
    }
}
