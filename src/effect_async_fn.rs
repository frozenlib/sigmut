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
pub fn effect_async(f: impl AsyncFnMut(&mut AsyncSignalContext) + 'static) -> Subscription {
    effect_async_in(f, ReactionPhase::default())
}

/// Call an asynchronous function each time a dependency changes with `ReactionPhase` specified.
#[allow(clippy::await_holding_refcell_ref)]
pub fn effect_async_in(
    f: impl AsyncFnMut(&mut AsyncSignalContext) + 'static,
    phase: ReactionPhase,
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
    fut: Pin<Box<Option<Fut>>>,
    asb: AsyncSourceBinder,
}

struct EffectAsyncNode<GetFut, Fut> {
    data: RefCell<EffectAsyncData<GetFut, Fut>>,
    phase: ReactionPhase,
}
impl<GetFut, Fut> EffectAsyncNode<GetFut, Fut>
where
    GetFut: FnMut(AsyncSignalContext) -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    fn new(f: GetFut, phase: ReactionPhase) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            data: RefCell::new(EffectAsyncData {
                get_fut: f,
                fut: Box::pin(None),
                asb: AsyncSourceBinder::new(this),
            }),
            phase,
        })
    }
    fn schedule(self: &Rc<Self>) {
        Reaction::from_weak_fn(Rc::downgrade(self), |this, rc| this.call(rc))
            .schedule_with(self.phase)
    }
    fn call(self: &Rc<Self>, rc: &mut ReactionContext) {
        let d = &mut *self.data.borrow_mut();
        if d.asb.is_clean() {
            return;
        }
        if d.asb.check(rc) {
            d.fut.set(None);
            d.fut.set(Some(d.asb.init(&mut d.get_fut, rc)));
        }
        if let Some(fut) = d.fut.as_mut().as_pin_mut()
            && d.asb.poll(fut, rc).is_ready()
        {
            d.fut.set(None);
        }
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
