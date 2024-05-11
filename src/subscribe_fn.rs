use std::{cell::RefCell, rc::Rc};

use crate::{
    core::{
        BindSink, Dirty, DirtyOrMaybeDirty, NotifyContext, Scheduler, Slot, SourceBindings, Task,
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
    let node = Rc::new(SubscribeNode::new(f, scheduler.clone()));
    node.schedule();
    Subscription::from_rc(node)
}

struct SubscribeNodeData<F> {
    f: F,
    dirty: Dirty,
    sources: SourceBindings,
}

struct SubscribeNode<F> {
    data: RefCell<SubscribeNodeData<F>>,
    scheduler: Scheduler,
}
impl<F> SubscribeNode<F>
where
    F: FnMut(&mut SignalContext) + 'static,
{
    fn new(f: F, scheduler: Scheduler) -> Self {
        Self {
            data: RefCell::new(SubscribeNodeData {
                f,
                dirty: Dirty::Dirty,
                sources: SourceBindings::new(),
            }),
            scheduler,
        }
    }

    fn schedule(self: &Rc<Self>) {
        Task::from_weak_fn(Rc::downgrade(self), Self::call).schedule_with(&self.scheduler)
    }
    fn call(self: Rc<Self>, uc: &mut UpdateContext) {
        let d = &mut *self.data.borrow_mut();
        if d.dirty.check(&mut d.sources, uc) {
            let sink = Rc::downgrade(&self);
            d.sources.update(sink, Slot(0), true, &mut d.f, uc);
            d.dirty = Dirty::Clean;
        }
    }
}

impl<F> BindSink for SubscribeNode<F>
where
    F: FnMut(&mut SignalContext) + 'static,
{
    fn notify(self: Rc<Self>, _slot: Slot, dirty: DirtyOrMaybeDirty, _nc: &mut NotifyContext) {
        let mut data = self.data.borrow_mut();
        if data.dirty.is_clean() {
            self.schedule();
        }
        data.dirty |= dirty;
    }
}
