use std::{cell::RefCell, future::Future, ops::AsyncFnMut, pin::Pin, rc::Rc};

use crate::{
    Subscription,
    core::{
        AsyncSignalContext, AsyncSourceBinder, BindSink, DirtyLevel, NotifyContext, Reaction,
        ReactionContext, ReactionPhase, Slot,
    },
};

#[cfg(test)]
mod tests;

/// Call an asynchronous function each time a dependency changes.
///
/// Its initial execution and pending updates survive Runtime shutdown, and resume
/// when a Running runtime supports the shared default phase.
pub fn effect_async(f: impl AsyncFnMut(&mut AsyncSignalContext) + 'static) -> Subscription {
    effect_async_in(ReactionPhase::DEFAULT, f)
}

/// Call an asynchronous function each time a dependency changes with [`ReactionPhase`] specified.
///
/// Unsupported phases sleep until a matching Running runtime exists. The
/// subscription owns the callback and its resources; dropping it disables queued
/// work. Runtime recreation does not replay a clean effect or restore external
/// resources captured by the callback.
#[allow(clippy::await_holding_refcell_ref)]
pub fn effect_async_in(
    phase: &'static ReactionPhase,
    f: impl AsyncFnMut(&mut AsyncSignalContext) + 'static,
) -> Subscription {
    let f = Rc::new(RefCell::new(f));
    let this = EffectAsyncNode::new(
        move |mut sc| {
            let f = f.clone();
            async move {
                f.borrow_mut()(&mut sc).await;
            }
        },
        phase,
    );
    this.schedule();
    Subscription::from_rc(this)
}

struct EffectAsyncData<GetFut, Fut> {
    get_fut: GetFut,
    fut: Option<Pin<Box<Fut>>>,
    asb: AsyncSourceBinder,
}

struct EffectAsyncNode<GetFut, Fut> {
    data: RefCell<EffectAsyncData<GetFut, Fut>>,
    phase: &'static ReactionPhase,
}
impl<GetFut, Fut> EffectAsyncNode<GetFut, Fut>
where
    GetFut: FnMut(AsyncSignalContext) -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    fn new(f: GetFut, phase: &'static ReactionPhase) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            data: RefCell::new(EffectAsyncData {
                get_fut: f,
                fut: None,
                asb: AsyncSourceBinder::new(this),
            }),
            phase,
        })
    }
    fn schedule(self: &Rc<Self>) {
        Reaction::from_weak_fn(Rc::downgrade(self), |this, rc| this.call(rc))
            .schedule_in(self.phase)
    }
    fn call(self: &Rc<Self>, rc: &mut ReactionContext) {
        // Declare the detached future before the borrow so unwind also drops
        // the borrow first. Pins stay attached to their heap allocation.
        let mut future;
        let mut data = self.data.borrow_mut();
        if data.asb.is_clean() {
            return;
        }
        future = data.fut.take();
        if data.asb.check(rc) {
            drop(data);
            drop(future);
            data = self.data.borrow_mut();
            let EffectAsyncData { asb, get_fut, .. } = &mut *data;
            future = Some(Box::pin(asb.init(get_fut, rc)));
        }
        let complete = future
            .as_mut()
            .is_some_and(|future| data.asb.poll(future.as_mut(), rc).is_ready());
        if !complete {
            data.fut = future.take();
        }
        drop(data);
        drop(future);
    }
}
impl<F, Fut> BindSink for EffectAsyncNode<F, Fut>
where
    F: FnMut(AsyncSignalContext) -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    fn notify(self: Rc<Self>, slot: Slot, level: DirtyLevel, _nc: &mut NotifyContext) {
        if self.data.borrow_mut().asb.on_notify(slot, level) {
            self.schedule();
        }
    }
}
