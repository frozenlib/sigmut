use std::{cell::RefCell, rc::Rc};

use crate::{
    SignalContext, Subscription,
    core::{
        BindSink, DirtyLevel, NotifyContext, Reaction, ReactionContext, ReactionKind, Slot,
        SourceBinder,
    },
};

#[cfg(test)]
mod tests;

/// Call a function each time a dependency changes.
///
/// The function is called when [`Runtime::dispatch_reactions`](crate::core::Runtime::dispatch_reactions)
/// is called with `ReactionKind::default()`,
/// or when [`Runtime::dispatch_all_reactions`](crate::core::Runtime::dispatch_all_reactions) is called.
/// However, if the dependency status has not changed since the previous call, it will not be called.
///
/// If the [`Subscription`] returned from this function is dropped, the function will not be called again.
pub fn effect(f: impl FnMut(&mut SignalContext) + 'static) -> Subscription {
    effect_with(f, ReactionKind::default())
}

/// Call a function each time a dependency changes with [`ReactionKind`] specified.
///
/// The function is called when [`Runtime::dispatch_reactions`](crate::core::Runtime::dispatch_reactions)
/// is called with `kind`,
/// or when [`Runtime::dispatch_all_reactions`](crate::core::Runtime::dispatch_all_reactions) is called.
/// However, if the dependency status has not changed since the previous call, it will not be called.
///
/// If the [`Subscription`] returned from this function is dropped, the function will not be called again.
pub fn effect_with(
    f: impl FnMut(&mut SignalContext) + 'static,
    kind: ReactionKind,
) -> Subscription {
    let node = EffectNode::new(f, kind);
    node.schedule();
    Subscription::from_rc(node)
}

struct EffectData<F> {
    f: F,
    sb: SourceBinder,
}

struct EffectNode<F> {
    data: RefCell<EffectData<F>>,
    kind: ReactionKind,
}
impl<F> EffectNode<F>
where
    F: FnMut(&mut SignalContext) + 'static,
{
    fn new(f: F, kind: ReactionKind) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            data: RefCell::new(EffectData {
                f,
                sb: SourceBinder::new(this, Slot(0)),
            }),
            kind,
        })
    }

    fn schedule(self: &Rc<Self>) {
        Reaction::from_weak_fn(Rc::downgrade(self), Self::call).schedule_with(self.kind)
    }
    fn call(self: Rc<Self>, rc: &mut ReactionContext) {
        let d = &mut *self.data.borrow_mut();
        if d.sb.check(rc) {
            d.sb.update(&mut d.f, rc);
        }
    }
}

impl<F> BindSink for EffectNode<F>
where
    F: FnMut(&mut SignalContext) + 'static,
{
    fn notify(self: Rc<Self>, slot: Slot, level: DirtyLevel, _nc: &mut NotifyContext) {
        if self.data.borrow_mut().sb.on_notify(slot, level) {
            self.schedule();
        }
    }
}
