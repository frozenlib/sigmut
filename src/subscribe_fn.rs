use std::{cell::RefCell, rc::Rc};

use crate::{
    core::{
        BindSink, DirtyOrMaybeDirty, NotifyContext, Scheduler, Slot, SourceBinder, Task,
        UpdateContext,
    },
    SignalContext, Subscription,
};

pub fn subscribe(f: impl FnMut(&mut SignalContext) + 'static) -> Subscription {
    subscribe_with(f, &Scheduler::default())
}
pub fn subscribe_with(
    f: impl FnMut(&mut SignalContext) + 'static,
    scheduler: &Scheduler,
) -> Subscription {
    let node = SubscribeNode::new(f, scheduler.clone());
    node.schedule();
    Subscription::from_rc(node)
}

struct SubscribeNodeData<F> {
    f: F,
    sb: SourceBinder,
}

struct SubscribeNode<F> {
    data: RefCell<SubscribeNodeData<F>>,
    scheduler: Scheduler,
}
impl<F> SubscribeNode<F>
where
    F: FnMut(&mut SignalContext) + 'static,
{
    fn new(f: F, scheduler: Scheduler) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            data: RefCell::new(SubscribeNodeData {
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

impl<F> BindSink for SubscribeNode<F>
where
    F: FnMut(&mut SignalContext) + 'static,
{
    fn notify(self: Rc<Self>, slot: Slot, dirty: DirtyOrMaybeDirty, _nc: &mut NotifyContext) {
        if self.data.borrow_mut().sb.on_notify(slot, dirty) {
            self.schedule();
        }
    }
}
