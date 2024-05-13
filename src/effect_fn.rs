use std::{cell::RefCell, rc::Rc};

use crate::{
    core::{
        BindSink, DirtyOrMaybeDirty, NotifyContext, Scheduler, Slot, SourceBinder, Task,
        UpdateContext,
    },
    SignalContext, Subscription,
};

pub fn effect(f: impl FnMut(&mut SignalContext) + 'static) -> Subscription {
    effect_with(f, &Scheduler::default())
}
pub fn effect_with(
    f: impl FnMut(&mut SignalContext) + 'static,
    scheduler: &Scheduler,
) -> Subscription {
    let node = EffectNode::new(f, scheduler.clone());
    node.schedule();
    Subscription::from_rc(node)
}

struct EffectData<F> {
    f: F,
    sb: SourceBinder,
}

struct EffectNode<F> {
    data: RefCell<EffectData<F>>,
    scheduler: Scheduler,
}
impl<F> EffectNode<F>
where
    F: FnMut(&mut SignalContext) + 'static,
{
    fn new(f: F, scheduler: Scheduler) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            data: RefCell::new(EffectData {
                f,
                sb: SourceBinder::new(this, Slot(0)),
            }),
            scheduler,
        })
    }

    fn schedule(self: &Rc<Self>) {
        Task::from_weak_fn(Rc::downgrade(self), Self::call).schedule_with(&self.scheduler)
    }
    fn call(self: Rc<Self>, uc: &mut UpdateContext) {
        let d = &mut *self.data.borrow_mut();
        if d.sb.check(uc) {
            d.sb.update(&mut d.f, uc);
        }
    }
}

impl<F> BindSink for EffectNode<F>
where
    F: FnMut(&mut SignalContext) + 'static,
{
    fn notify(self: Rc<Self>, slot: Slot, dirty: DirtyOrMaybeDirty, _nc: &mut NotifyContext) {
        if self.data.borrow_mut().sb.on_notify(slot, dirty) {
            self.schedule();
        }
    }
}
