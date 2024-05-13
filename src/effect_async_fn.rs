use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc};

use crate::{
    core::{
        AsyncSignalContext, AsyncSourceBinder, BindSink, DirtyOrMaybeDirty, NotifyContext, Slot,
        Task, UpdateContext,
    },
    Scheduler, Subscription,
};

pub fn effect_async<Fut>(f: impl FnMut(AsyncSignalContext) -> Fut + 'static) -> Subscription
where
    Fut: Future<Output = ()> + 'static,
{
    let this = EffectAsyncNode::new(f, Scheduler::default());
    this.schedule();
    Subscription::from_rc(this)
}

struct EffectAsyncData<GetFut, Fut> {
    get_fut: GetFut,
    fut: Pin<Box<Option<Fut>>>,
    asb: AsyncSourceBinder,
}

struct EffectAsyncNode<GetFut, Fut> {
    data: RefCell<EffectAsyncData<GetFut, Fut>>,
    scheduler: Scheduler,
}
impl<GetFut, Fut> EffectAsyncNode<GetFut, Fut>
where
    GetFut: FnMut(AsyncSignalContext) -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    fn new(f: GetFut, scheduler: Scheduler) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            data: RefCell::new(EffectAsyncData {
                get_fut: f,
                fut: Box::pin(None),
                asb: AsyncSourceBinder::new(this),
            }),
            scheduler,
        })
    }
    fn schedule(self: &Rc<Self>) {
        Task::from_weak_fn(Rc::downgrade(self), |this, uc| this.call(uc))
            .schedule_with(&self.scheduler)
    }
    fn call(self: &Rc<Self>, uc: &mut UpdateContext) {
        let d = &mut *self.data.borrow_mut();
        if d.asb.is_clean() {
            return;
        }
        if d.asb.check(uc) {
            d.fut.set(None);
            d.fut.set(Some(d.asb.init(&mut d.get_fut, uc)));
        }
        if let Some(fut) = d.fut.as_mut().as_pin_mut() {
            if d.asb.poll(fut, uc).is_ready() {
                d.fut.set(None);
            }
        }
    }
}
impl<F, Fut> BindSink for EffectAsyncNode<F, Fut>
where
    F: FnMut(AsyncSignalContext) -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    fn notify(self: Rc<Self>, slot: Slot, dirty: DirtyOrMaybeDirty, _nc: &mut NotifyContext) {
        if self.data.borrow_mut().asb.on_notify(slot, dirty) {
            self.schedule();
        }
    }
}
