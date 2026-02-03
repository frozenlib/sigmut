use std::{cell::RefCell, rc::Rc};

use crate::{
    SignalContext, Subscription,
    core::{
        BindSink, DirtyLevel, NotifyContext, Reaction, ReactionContext, ReactionPhase, Slot,
        SourceBinder,
    },
};

#[cfg(test)]
mod tests;

/// Call a function each time a dependency changes.
///
/// The function is called when [`Runtime::dispatch_reactions`](crate::core::Runtime::dispatch_reactions)
/// is called with `ReactionPhase::default()`,
/// or when [`Runtime::dispatch_all_reactions`](crate::core::Runtime::dispatch_all_reactions) is called.
/// However, if the dependency status has not changed since the previous call, it will not be called.
///
/// If the [`Subscription`] returned from this function is dropped, the function will not be called again.
pub fn effect(f: impl FnMut(&mut SignalContext<'_, '_>) + 'static) -> Subscription {
    effect_in(ReactionPhase::default(), f)
}

/// Call a function each time a dependency changes with [`ReactionPhase`] specified.
///
/// The function is called when [`Runtime::dispatch_reactions`](crate::core::Runtime::dispatch_reactions)
/// is called with `phase`,
/// or when [`Runtime::dispatch_all_reactions`](crate::core::Runtime::dispatch_all_reactions) is called.
/// However, if the dependency status has not changed since the previous call, it will not be called.
///
/// If the [`Subscription`] returned from this function is dropped, the function will not be called again.
pub fn effect_in(
    phase: ReactionPhase,
    f: impl FnMut(&mut SignalContext<'_, '_>) + 'static,
) -> Subscription {
    let node = EffectNode::new(f, phase);
    node.schedule();
    Subscription::from_rc(node)
}

struct EffectData<F> {
    f: F,
    sb: SourceBinder,
}

struct EffectNode<F> {
    data: RefCell<EffectData<F>>,
    phase: ReactionPhase,
}
impl<F> EffectNode<F>
where
    F: FnMut(&mut SignalContext<'_, '_>) + 'static,
{
    fn new(f: F, phase: ReactionPhase) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            data: RefCell::new(EffectData {
                f,
                sb: SourceBinder::new(this, Slot(0)),
            }),
            phase,
        })
    }

    fn schedule(self: &Rc<Self>) {
        Reaction::from_weak_fn(Rc::downgrade(self), Self::call).schedule_in(self.phase)
    }
    fn call(self: Rc<Self>, rc: &mut ReactionContext<'_, '_>) {
        let d = &mut *self.data.borrow_mut();
        if d.sb.check(rc) {
            d.sb.update(&mut d.f, rc);
        }
    }
}

impl<F> BindSink for EffectNode<F>
where
    F: FnMut(&mut SignalContext<'_, '_>) + 'static,
{
    fn notify(self: Rc<Self>, slot: Slot, level: DirtyLevel, _nc: &mut NotifyContext) {
        if self.data.borrow_mut().sb.on_notify(slot, level) {
            self.schedule();
        }
    }
}
