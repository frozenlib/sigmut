use std::{cell::Cell, marker::PhantomData, rc::Rc};

use super::{
    ActionContext, ReactionContext, SignalContext,
    raw_context::{ActionContextPtr, SignalContextPtr},
};

#[cfg(test)]
mod tests;

#[derive(Copy, Clone)]
enum ChannelState<P> {
    Inactive,
    Available(P),
    Borrowed,
}

struct ContextChannel<P: Copy> {
    state: Cell<ChannelState<P>>,
    not_send_or_sync: PhantomData<Rc<()>>,
}

impl<P: Copy> ContextChannel<P> {
    const fn new() -> Self {
        Self {
            state: Cell::new(ChannelState::Inactive),
            not_send_or_sync: PhantomData,
        }
    }

    fn scope<T>(&self, pointer: P, f: impl FnOnce() -> T) -> T {
        let previous = self.state.replace(ChannelState::Available(pointer));
        let _guard = StateGuard {
            state: &self.state,
            restore: previous,
        };
        f()
    }

    fn try_borrow(&self, borrowed_message: &'static str) -> Option<(P, StateGuard<'_, P>)> {
        match self.state.replace(ChannelState::Borrowed) {
            ChannelState::Inactive => {
                self.state.set(ChannelState::Inactive);
                None
            }
            ChannelState::Available(pointer) => Some((
                pointer,
                StateGuard {
                    state: &self.state,
                    restore: ChannelState::Available(pointer),
                },
            )),
            ChannelState::Borrowed => panic!("{borrowed_message}"),
        }
    }
}

struct StateGuard<'a, P: Copy> {
    state: &'a Cell<ChannelState<P>>,
    restore: ChannelState<P>,
}

impl<P: Copy> Drop for StateGuard<'_, P> {
    fn drop(&mut self) {
        self.state.set(self.restore);
    }
}

/// Temporarily transfers a [`SignalContext`] through synchronous callbacks.
///
/// `scope` describes the lexical period in which a context is available, while `try_with`
/// distinguishes an inactive channel from an invalid recursive borrow. A channel is inline and
/// does not allocate. It is neither `Send` nor `Sync`, matching sigmut's thread-local runtime.
///
/// # Examples
///
/// ```
/// use sigmut::{SignalContextChannel, State, core::Runtime};
///
/// fn native_callback(channel: &SignalContextChannel, state: &State<i32>) -> i32 {
///     channel.try_with(|context| state.get(context)).unwrap()
/// }
///
/// let channel = SignalContextChannel::new();
/// let state = State::new(42);
/// let mut runtime = Runtime::new();
/// let mut context = runtime.sc();
///
/// let value = channel.scope(&mut context, || native_callback(&channel, &state));
/// assert_eq!(value, 42);
/// ```
#[must_use]
pub struct SignalContextChannel(ContextChannel<SignalContextPtr>);

impl SignalContextChannel {
    /// Creates an inactive signal context channel.
    pub const fn new() -> Self {
        Self(ContextChannel::new())
    }

    /// Makes `context` available while `f` runs.
    ///
    /// Scopes may be nested. The previous state is restored when `f` returns or unwinds. In
    /// particular, calling `scope` with the context received by [`try_with`](Self::try_with)
    /// explicitly reborrows it for a synchronously reentered callback.
    pub fn scope<T>(&self, context: &mut SignalContext<'_, '_>, f: impl FnOnce() -> T) -> T {
        self.0.scope(SignalContextPtr::new(context), f)
    }

    /// Calls `f` with the currently available signal context.
    ///
    /// Returns `None` without calling `f` when the channel is inactive. The context lifetimes are
    /// local to `f`, so values such as [`StateRef`](super::StateRef) cannot escape the callback.
    ///
    /// # Panics
    ///
    /// Panics if the context is already borrowed by another `try_with` callback. Use
    /// [`scope`](Self::scope) to explicitly reborrow that callback's context before synchronous
    /// reentry.
    ///
    /// ```compile_fail
    /// use sigmut::{SignalContextChannel, State, StateRef};
    ///
    /// fn escape<'a>(
    ///     channel: &SignalContextChannel,
    ///     state: &'a State<i32>,
    /// ) -> StateRef<'a, i32> {
    ///     channel.try_with(|context| state.borrow(context)).unwrap()
    /// }
    /// ```
    pub fn try_with<T>(
        &self,
        f: impl for<'a, 'r, 's> FnOnce(&'a mut SignalContext<'r, 's>) -> T,
    ) -> Option<T> {
        let (pointer, _guard) = self.0.try_borrow(
            "`SignalContextChannel::try_with` cannot borrow a context that is already borrowed",
        )?;
        let mut context = unsafe { pointer.signal_context() };
        Some(f(&mut context))
    }
}

impl Default for SignalContextChannel {
    fn default() -> Self {
        Self::new()
    }
}

/// Temporarily transfers a [`ReactionContext`] through synchronous callbacks.
///
/// This channel preserves reaction contexts' untracked, non-mutating authority. `scope` describes
/// the lexical period in which a context is available, while `try_with` distinguishes an inactive
/// channel from an invalid recursive borrow. A channel is inline and does not allocate. It is
/// neither `Send` nor `Sync`, matching sigmut's thread-local runtime.
///
/// # Examples
///
/// ```
/// use std::cell::RefCell;
/// use sigmut::{ReactionContextChannel, core::Runtime};
///
/// let channel = ReactionContextChannel::new();
/// let value = RefCell::new(42);
/// let mut runtime = Runtime::new();
/// let mut context = runtime.rc();
///
/// let current = channel.scope(&mut context, || {
///     channel.try_with(|context| *context.borrow(&value)).unwrap()
/// });
/// assert_eq!(current, 42);
/// ```
#[must_use]
pub struct ReactionContextChannel(ContextChannel<SignalContextPtr>);

impl ReactionContextChannel {
    /// Creates an inactive reaction context channel.
    pub const fn new() -> Self {
        Self(ContextChannel::new())
    }

    /// Makes `context` available while `f` runs.
    ///
    /// Scopes may be nested. The previous state is restored when `f` returns or unwinds. In
    /// particular, calling `scope` with the context received by [`try_with`](Self::try_with)
    /// explicitly reborrows it for a synchronously reentered callback.
    pub fn scope<T>(&self, context: &mut ReactionContext<'_, '_>, f: impl FnOnce() -> T) -> T {
        self.0.scope(SignalContextPtr::from_reaction(context), f)
    }

    /// Calls `f` with the currently available reaction context.
    ///
    /// Returns `None` without calling `f` when the channel is inactive. The context lifetimes are
    /// local to `f`, so values tied to its temporary signal context cannot escape the callback.
    ///
    /// # Panics
    ///
    /// Panics if the context is already borrowed by another `try_with` callback. Use
    /// [`scope`](Self::scope) to explicitly reborrow that callback's context before synchronous
    /// reentry.
    ///
    /// ```compile_fail
    /// use sigmut::{ReactionContextChannel, State, StateRef};
    ///
    /// fn escape<'a>(
    ///     channel: &ReactionContextChannel,
    ///     state: &'a State<i32>,
    /// ) -> StateRef<'a, i32> {
    ///     channel
    ///         .try_with(|context| context.sc_with(|context| state.borrow(context)))
    ///         .unwrap()
    /// }
    /// ```
    pub fn try_with<T>(
        &self,
        f: impl for<'a, 'r, 's> FnOnce(&'a mut ReactionContext<'r, 's>) -> T,
    ) -> Option<T> {
        let (pointer, _guard) = self.0.try_borrow(
            "`ReactionContextChannel::try_with` cannot borrow a context that is already borrowed",
        )?;
        let mut context = unsafe { pointer.reaction_context() };
        Some(f(&mut context))
    }
}

impl Default for ReactionContextChannel {
    fn default() -> Self {
        Self::new()
    }
}

/// Temporarily transfers an [`ActionContext`] through synchronous callbacks.
///
/// This channel preserves action contexts' mutation authority. `scope` describes the lexical
/// period in which a context is available, while `try_with` distinguishes an inactive channel
/// from an invalid recursive borrow. A channel is inline and does not allocate. It is neither
/// `Send` nor `Sync`, matching sigmut's thread-local runtime.
///
/// # Examples
///
/// ```
/// use sigmut::{ActionContextChannel, State, core::Runtime};
///
/// let channel = ActionContextChannel::new();
/// let state = State::new(0);
/// let mut runtime = Runtime::new();
///
/// channel.scope(runtime.ac(), || {
///     channel.try_with(|context| state.set(42, context)).unwrap();
/// });
/// assert_eq!(state.get(&mut runtime.sc()), 42);
/// ```
#[must_use]
pub struct ActionContextChannel(ContextChannel<ActionContextPtr>);

impl ActionContextChannel {
    /// Creates an inactive action context channel.
    pub const fn new() -> Self {
        Self(ContextChannel::new())
    }

    /// Makes `context` available while `f` runs.
    ///
    /// Scopes may be nested. The previous state is restored when `f` returns or unwinds. In
    /// particular, calling `scope` with the context received by [`try_with`](Self::try_with)
    /// explicitly reborrows it for a synchronously reentered callback.
    pub fn scope<T>(&self, context: &mut ActionContext, f: impl FnOnce() -> T) -> T {
        self.0.scope(ActionContextPtr::new(context), f)
    }

    /// Calls `f` with the currently available action context.
    ///
    /// Returns `None` without calling `f` when the channel is inactive. The mutable context borrow
    /// is local to `f`, so values tied to it cannot escape the callback.
    ///
    /// # Panics
    ///
    /// Panics if the context is already borrowed by another `try_with` callback. Use
    /// [`scope`](Self::scope) to explicitly reborrow that callback's context before synchronous
    /// reentry.
    ///
    /// ```compile_fail
    /// use sigmut::{ActionContextChannel, State, state::StateRefMut};
    ///
    /// fn escape<'a>(
    ///     channel: &ActionContextChannel,
    ///     state: &'a State<i32>,
    /// ) -> StateRefMut<'a, i32> {
    ///     channel
    ///         .try_with(|context| state.borrow_mut(context))
    ///         .unwrap()
    /// }
    /// ```
    pub fn try_with<T>(&self, f: impl for<'a> FnOnce(&'a mut ActionContext) -> T) -> Option<T> {
        let (pointer, _guard) = self.0.try_borrow(
            "`ActionContextChannel::try_with` cannot borrow a context that is already borrowed",
        )?;
        Some(f(unsafe { pointer.action_context() }))
    }
}

impl Default for ActionContextChannel {
    fn default() -> Self {
        Self::new()
    }
}
