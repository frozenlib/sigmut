use bumpalo::Bump;

use super::{ActionContext, RawRuntime, ReactionContext, RuntimeData, SignalContext, Sink};

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub(super) struct SignalContextPtr {
    rt: *mut RuntimeData,
    bump: *const Bump,
    sink: Option<*mut Sink>,
}

impl SignalContextPtr {
    pub(super) fn new(context: &mut SignalContext<'_, '_>) -> Self {
        Self {
            rt: context.rt,
            bump: context.bump,
            sink: context.sink.as_mut().map(|sink| *sink as *mut _),
        }
    }

    pub(super) fn from_reaction(context: &mut ReactionContext<'_, '_>) -> Self {
        Self::new(&mut context.0)
    }

    /// Reconstructs a signal context with caller-bounded lifetimes.
    ///
    /// # Safety
    ///
    /// The source context must remain exclusively borrowed for both returned lifetimes, and the
    /// returned context must not outlive the source context.
    pub(super) unsafe fn signal_context<'r, 's>(self) -> SignalContext<'r, 's> {
        SignalContext {
            rt: unsafe { &mut *self.rt },
            bump: unsafe { &*self.bump },
            sink: self.sink.map(|sink| unsafe { &mut *sink }),
        }
    }

    /// Reconstructs a reaction context with caller-bounded lifetimes.
    ///
    /// # Safety
    ///
    /// The source context must remain exclusively borrowed for both returned lifetimes, and the
    /// returned context must not outlive the source context.
    pub(super) unsafe fn reaction_context<'r, 's>(self) -> ReactionContext<'r, 's> {
        ReactionContext(unsafe { self.signal_context() })
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub(super) struct ActionContextPtr(*mut RawRuntime);

impl ActionContextPtr {
    pub(super) fn new(context: &mut ActionContext) -> Self {
        Self(&mut context.0)
    }

    /// Reconstructs an action context with a caller-bounded lifetime.
    ///
    /// # Safety
    ///
    /// The source context must remain exclusively borrowed for the returned lifetime, and the
    /// returned context must not outlive the source context.
    pub(super) unsafe fn action_context<'a>(self) -> &'a mut ActionContext {
        ActionContext::new(unsafe { &mut *self.0 })
    }
}
