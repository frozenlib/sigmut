use super::from_async::FromAsync;
use crate::core::{BindSink, CallUpdate, Computed, ObsContext, Runtime, SourceBindings};
use derive_ex::derive_ex;
use futures::Future;
use std::{any::Any, cell::RefCell, rc::Rc};

const PARAM: usize = 0;

#[derive_ex(Default)]
#[default(Self::empty())]
pub struct Subscription(Option<Rc<dyn Any>>);

impl Subscription {
    pub fn new(f: impl FnMut(&mut ObsContext) + 'static) -> Self {
        let rc = Rc::new(RawSubscription(RefCell::new(Data {
            f,
            is_scheduled_update: false,
            computed: Computed::None,
            bindings: SourceBindings::new(),
        })));
        let node = Rc::downgrade(&rc);
        Runtime::schedule_update_lazy(node, PARAM);
        Self(Some(rc))
    }
    pub fn new_async<Fut>(f: impl FnMut(&mut ObsContext) -> Fut + 'static) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        let mut f = f;
        Subscription(Some(FromAsync::new(move |mut oc| oc.oc_with(&mut f), true)))
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

impl<F: FnMut(&mut ObsContext) + 'static> BindSink for RawSubscription<F> {
    fn notify(self: Rc<Self>, _param: usize, is_modified: bool, rt: &mut Runtime) {
        let mut is_schedule = false;
        if let Ok(mut d) = self.0.try_borrow_mut() {
            if d.computed.modify(is_modified) && !d.is_scheduled_update {
                d.is_scheduled_update = true;
                is_schedule = true;
            }
        }
        if is_schedule {
            rt.schedule_update(self, PARAM);
        }
    }
}

impl<F: FnMut(&mut ObsContext) + 'static> CallUpdate for RawSubscription<F> {
    fn call_update(self: Rc<Self>, _param: usize, rt: &mut Runtime) {
        let mut d = self.0.borrow_mut();
        let d = &mut *d;
        d.is_scheduled_update = false;
        if d.computed == Computed::MayBeOutdated {
            if d.bindings.flush(rt) {
                d.computed = Computed::Outdated;
            } else {
                d.computed = Computed::UpToDate;
            }
        }
        if d.computed != Computed::UpToDate {
            d.computed = Computed::UpToDate;
            let node = Rc::downgrade(&self);
            d.bindings.compute(node, PARAM, |cc| (d.f)(cc.oc()), rt);
        }
    }
}
