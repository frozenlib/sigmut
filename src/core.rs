use core::panic;
use std::{
    any::Any,
    cell::{Cell, Ref, RefCell},
    collections::VecDeque,
    future::{Future, poll_fn},
    mem::{replace, swap, take, transmute},
    ops::AsyncFnOnce,
    pin::Pin,
    rc::{Rc, Weak},
    result::Result,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
    thread::AccessError,
};

use bumpalo::Bump;
use derive_ex::Ex;
use parse_display::Display;
use slabmap::SlabMap;

mod async_signal_context;
mod context_channel;
mod dirty;
mod phases;
use phases::PhaseOrder;
pub use phases::{ActionPhase, ReactionPhase};
mod raw_context;
mod source_binder;
mod state_ref;
mod state_ref_builder;

pub use async_signal_context::*;
pub use context_channel::*;
pub use dirty::*;
pub use source_binder::SourceBinder;
pub use state_ref::StateRef;
pub use state_ref_builder::StateRefBuilder;

use crate::utils::downcast_or;

thread_local! {
    static GLOBALS: RefCell<Globals> = RefCell::new(Globals::new());
}

struct Globals {
    runtime_config: Option<RuntimeConfig>,
    closing: bool,
    runtime: Option<Box<RawRuntime>>,
    unbinds: Vec<SourceBindingsData>,
    actions: Vec<VecDeque<Action>>,
    notifys: Vec<NotifyReaction>,
    wakes: WakeTable,
    reactions: Vec<VecDeque<Reaction>>,
    runnable_reactions: usize,
    discards: Vec<Reaction>,
    wake_driver: Option<Waker>,
}
impl Globals {
    fn new() -> Self {
        Self {
            runtime_config: None,
            closing: false,
            runtime: None,
            unbinds: Vec::new(),
            actions: Vec::new(),
            notifys: Vec::new(),
            wakes: WakeTable::default(),
            reactions: Vec::new(),
            runnable_reactions: 0,
            discards: Vec::new(),
            wake_driver: None,
        }
    }
    fn with<T>(f: impl FnOnce(&mut Self) -> T) -> T {
        Self::try_with(f).expect("sigmut thread local storage is unavailable")
    }
    fn try_with<T>(f: impl FnOnce(&mut Self) -> T) -> Result<T, AccessError> {
        let (value, wake) = GLOBALS.try_with(|g| {
            let mut g = g.borrow_mut();
            let value = f(&mut g);
            (value, g.wake_driver.take())
        })?;
        // User wakers may reenter sigmut. Never invoke them under a TLS borrow.
        if let Some(wake) = wake {
            wake.wake();
        }
        Ok(value)
    }
    fn prepare_runtime() {
        Self::with(|g| {
            assert!(
                g.runtime_config.is_none(),
                "Only one `Runtime` can exist in the same thread at the same time."
            )
        });
    }
    fn activate_runtime(config: RuntimeConfig) {
        Self::with(|g| {
            g.runnable_reactions = config
                .reaction_phases
                .ids
                .iter()
                .map(|&id| g.reactions.get(id).map_or(0, VecDeque::len))
                .sum();
            g.actions = (0..config.action_phases.ids.len())
                .map(|_| VecDeque::new())
                .collect();
            g.closing = false;
            g.runtime_config = Some(config);
        });
    }
    fn is_running() -> bool {
        Self::with(|g| g.runtime_config.is_some() && !g.closing)
    }
    fn action_phase_index(&self, phase: &'static ActionPhase) -> Option<usize> {
        let config = self
            .runtime_config
            .as_ref()
            .expect("Action requires a Runtime");
        if self.closing {
            return None;
        }
        Some(
            config
                .action_phases
                .index(phase.id())
                .expect("unsupported ActionPhase"),
        )
    }
    fn assert_valid_reaction_phase(phase: &'static ReactionPhase) {
        Self::with(|g| {
            g.runtime_config
                .as_ref()
                .unwrap()
                .assert_valid_reaction_phase(phase)
        });
    }
    fn schedule_reaction(phase: &'static ReactionPhase, reaction: Reaction) {
        let id = phase.id();
        Self::with(|g| {
            g.reactions
                .resize_with(g.reactions.len().max(id + 1), VecDeque::new);
            g.reactions[id].push_back(reaction);
            if g.runtime_config
                .as_ref()
                .is_some_and(|c| c.reaction_phases.index(id).is_some())
            {
                g.runnable_reactions += 1;
                if !g.closing {
                    g.wake();
                }
            }
        });
    }
    fn schedule_action(phase: &'static ActionPhase, action: Action) {
        // Keep rejected captures outside the TLS borrow, including during panic.
        let mut action = Some(action);
        Self::with(|g| {
            if let Some(index) = g.action_phase_index(phase) {
                g.actions[index].push_back(action.take().unwrap());
                g.wake();
            }
        });
        drop(action);
    }
    fn get_notifys(notifys: &mut Vec<NotifyReaction>) -> bool {
        Self::with(|g| {
            g.apply_wake();
            swap(notifys, &mut g.notifys);
        });
        !notifys.is_empty()
    }
    fn get_reaction_batch(phase: Option<&'static ReactionPhase>, batch: &mut Vec<(usize, usize)>) {
        Self::with(|g| {
            if g.closing {
                return;
            }
            let mut add = |id: usize| {
                let count = g.reactions.get(id).map_or(0, VecDeque::len);
                if count != 0 {
                    batch.push((id, count));
                }
            };
            if let Some(phase) = phase {
                add(phase.id());
            } else {
                for &id in &g.runtime_config.as_ref().unwrap().reaction_phases.ids {
                    add(id);
                }
            }
        });
    }
    fn get_action(phase: &'static ActionPhase) -> Option<Action> {
        Self::with(|g| {
            let index = g.action_phase_index(phase)?;
            g.apply_wake();
            g.actions[index].pop_front()
        })
    }
    fn get_action_batch(
        phase: Option<&'static ActionPhase>,
        batch: &mut Vec<(usize, usize)>,
    ) -> bool {
        Self::with(|g| {
            if g.closing {
                return false;
            }
            let index = phase.map(|phase| g.action_phase_index(phase).unwrap());
            g.apply_wake();
            let mut add = |index: usize| {
                let count = g.actions[index].len();
                if count != 0 {
                    batch.push((index, count));
                }
            };
            if let Some(index) = index {
                add(index);
            } else {
                for index in 0..g.actions.len() {
                    add(index);
                }
            }
            !batch.is_empty()
        })
    }
    fn swap_source_bindings(
        f: impl FnOnce(&mut Self) -> &mut Vec<SourceBindingsData>,
        values: &mut Vec<SourceBindingsData>,
    ) -> bool {
        Self::with(|g| swap(f(g), values));
        !values.is_empty()
    }
    fn push_unbinds(&mut self, bindings: SourceBindingsData) {
        self.unbinds.push(bindings);
        self.wake();
    }
    fn push_notify(&mut self, sink: Weak<dyn BindSink>, slot: Slot) {
        self.notifys.push(NotifyReaction { sink, slot });
        self.wake();
    }
    fn apply_wake(&mut self) {
        let (drops, wakes) = {
            let mut requests = self.wakes.requests.0.lock().unwrap();
            (take(&mut requests.drops), take(&mut requests.wakes))
        };
        for key in drops {
            self.wakes.reactions.remove(key);
        }
        for (key, active) in wakes {
            if !active.load(Ordering::Relaxed) {
                continue;
            }
            match self.wakes.reactions.get(key) {
                Some(WakeReaction::Notify(reaction)) => self.notifys.push(reaction.clone()),
                Some(WakeReaction::AsyncAction { action, index })
                    if !self.closing && action.strong_count() != 0 =>
                {
                    self.actions[*index]
                        .push_back(Action::from_weak_fn(action.clone(), AsyncAction::next));
                }
                _ => {}
            }
        }
    }
    fn has_ready_work(&self, requests: &RawWakeRequests) -> bool {
        !self.closing
            && (!self.notifys.is_empty()
                || self.actions.iter().any(|q| !q.is_empty())
                || self.runnable_reactions != 0
                || !self.unbinds.is_empty()
                || !self.discards.is_empty()
                || !requests.drops.is_empty()
                || !requests.wakes.is_empty())
    }
    fn wait_for_ready(waker: &Waker) -> Poll<()> {
        let (result, previous) = Self::with(|g| {
            g.apply_wake();
            let mut requests = g.wakes.requests.0.lock().unwrap();
            let ready = g.has_ready_work(&requests);
            let previous = replace(
                &mut requests.waker,
                (!g.closing && !ready).then(|| waker.clone()),
            );
            (
                if ready {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                },
                previous,
            )
        });
        drop(previous);
        result
    }
    fn begin_shutdown() -> (Vec<VecDeque<Action>>, Option<Waker>) {
        Self::with(|g| {
            g.closing = true;
            let waker = g.wakes.requests.0.lock().unwrap().waker.take();
            (take(&mut g.actions), waker)
        })
    }
    fn finish_runtime() {
        let waker = Self::with(|g| {
            g.runtime_config = None;
            g.runnable_reactions = 0;
            g.wakes.requests.0.lock().unwrap().waker.take()
        });
        drop(waker);
    }
    fn wake(&mut self) {
        if self.wake_driver.is_none() {
            self.wake_driver = self.wakes.requests.0.lock().unwrap().waker.take();
        }
    }
}

/// Ordered phases supported by a [`Runtime`].
///
/// The default supports only the shared default phase of each kind. Lists replace
/// the defaults, may be empty, and must not contain duplicates. Initialization
/// allocates a lookup table through the largest registered ID; subsequent lookups
/// are worst-case O(1), including unsupported IDs, and never grow that table.
///
/// ```
/// use sigmut::core::{ActionPhase, Runtime, RuntimeConfig};
/// static UPDATE: ActionPhase = ActionPhase::new();
/// let config = RuntimeConfig::default().with_action_phases([&UPDATE]);
/// let _runtime = Runtime::new_with_config(config);
/// ```
#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    action_phases: PhaseOrder,
    reaction_phases: PhaseOrder,
}
impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            action_phases: PhaseOrder::new([ActionPhase::DEFAULT.id()]),
            reaction_phases: PhaseOrder::new([ReactionPhase::DEFAULT.id()]),
        }
    }
}
impl RuntimeConfig {
    /// Sets the supported action phases in dispatch order, without adding defaults.
    ///
    /// # Panics
    /// Panics if a phase appears more than once.
    #[must_use]
    pub fn with_action_phases(
        mut self,
        phases: impl IntoIterator<Item = &'static ActionPhase>,
    ) -> Self {
        self.action_phases = PhaseOrder::new(phases.into_iter().map(ActionPhase::id));
        self
    }
    /// Sets the supported reaction phases in dispatch order, without adding defaults.
    ///
    /// # Panics
    /// Panics if a phase appears more than once.
    #[must_use]
    pub fn with_reaction_phases(
        mut self,
        phases: impl IntoIterator<Item = &'static ReactionPhase>,
    ) -> Self {
        self.reaction_phases = PhaseOrder::new(phases.into_iter().map(ReactionPhase::id));
        self
    }
    fn assert_valid_reaction_phase(&self, phase: &'static ReactionPhase) {
        assert!(
            self.reaction_phases.index(phase.id()).is_some(),
            "unsupported ReactionPhase"
        );
    }
}

/// Exclusive dispatcher and owner of the current thread's Action session.
///
/// A runtime starts Running and irreversibly enters Closing through
/// [`begin_shutdown`](Self::begin_shutdown) or Drop. Reactions and lifetime
/// maintenance persist in thread-local queues across runtimes; Actions do not.
/// Phases select executable work and do not isolate graphs. Shared default-phase
/// subscriptions may therefore execute under any default runtime on this thread.
#[derive(Ex)]
#[derive_ex(Default)]
#[default(Self::new())]
pub struct Runtime {
    is_owned: bool,
    raw: Option<Box<RawRuntime>>,
}

/// An error returned when [`Runtime::try_call`] cannot call the function.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Display)]
pub enum RuntimeCallError {
    /// No [`Runtime`] exists in the current thread.
    #[display("Runtime does not exist")]
    RuntimeDoesNotExist,
    /// The [`Runtime`] exists but is not currently available for calling.
    #[display("Runtime is not available. Ensure you are within a `Runtime::lend` call.")]
    RuntimeUnavailable,
}

impl std::error::Error for RuntimeCallError {}

impl Runtime {
    /// Creates a runtime supporting only the shared default phase of each kind.
    ///
    /// # Panics
    ///
    /// Panics if another [`Runtime`] exists in the current thread.
    pub fn new() -> Self {
        Self::new_with_config(RuntimeConfig::default())
    }

    /// Creates a runtime with the specified configuration.
    ///
    /// Reactions may be queued at any time. Only configured phases are resumed,
    /// in list order; unsupported phases remain dormant without making the runtime ready.
    ///
    /// # Panics
    ///
    /// Panics if another [`Runtime`] exists in the current thread.
    pub fn new_with_config(config: RuntimeConfig) -> Self {
        Globals::prepare_runtime();
        let raw = Box::new(RawRuntime {
            rt: RuntimeData::new(),
            bump: Bump::new(),
            notifys_buffer: Vec::new(),
            batch_buffer: Vec::new(),
            unbinds_buffer: Vec::new(),
        });
        Globals::activate_runtime(config);
        Self {
            is_owned: true,
            raw: Some(raw),
        }
    }

    /// Irreversibly stops dispatch and cancels all current and future Actions.
    ///
    /// Call from the control side between dispatches. Repeated calls are harmless.
    /// Cancellation drops captures and futures outside scheduler borrows, with the
    /// Closing binding and async cancellation context intact. State changes already
    /// applied are not rolled back. Reactions and maintenance remain in TLS for a
    /// later matching runtime. Reads of Signals holding futures return their
    /// stored values without polling in Closing. Drop performs the same transition
    /// automatically.
    pub fn begin_shutdown(&mut self) {
        let pending = Globals::begin_shutdown();
        drop(pending);
        self.as_raw().cancel_async_actions();
    }

    fn as_raw(&mut self) -> &mut RawRuntime {
        self.raw
            .as_mut()
            .expect("Runtime is unavailable. `Runtime::wait_for_ready` may have leaked.")
    }

    pub fn ac(&mut self) -> &mut ActionContext {
        self.as_raw().ac()
    }
    pub fn rc(&mut self) -> ReactionContext<'_, '_> {
        self.as_raw().rc()
    }
    pub fn sc(&mut self) -> SignalContext<'_, '_> {
        self.as_raw().sc()
    }

    /// Dispatches the first scheduled action for the specified phase.
    ///
    /// Actions scheduled while the dispatched action is running remain pending until a later
    /// dispatch. Actions for other phases are not dispatched.
    ///
    /// Returns `true` if an action was dispatched.
    ///
    /// # Panics
    ///
    /// Panics if `phase` is not valid according to the runtime's [`RuntimeConfig`].
    pub fn dispatch_action(&mut self, phase: &'static ActionPhase) -> bool {
        self.as_raw().dispatch_action(phase)
    }

    /// Dispatches all scheduled actions for the specified phase.
    ///
    /// This includes actions scheduled for the same phase while dispatching. Actions for other
    /// phases are not dispatched.
    ///
    /// Returns `true` if at least one action was dispatched.
    ///
    /// # Panics
    ///
    /// Panics if `phase` is not valid according to the runtime's [`RuntimeConfig`].
    pub fn dispatch_actions(&mut self, phase: &'static ActionPhase) -> bool {
        self.as_raw().dispatch_actions_with(Some(phase))
    }

    /// Dispatches all scheduled actions for all phases.
    ///
    /// This includes actions scheduled while dispatching.
    ///
    /// Returns `true` if at least one action was dispatched.
    pub fn dispatch_all_actions(&mut self) -> bool {
        self.as_raw().dispatch_actions_with(None)
    }

    /// Dispatch scheduled reactions for the specified phase.
    ///
    /// Returns `true` if any reaction was dispatched.
    ///
    /// # Panics
    ///
    /// Panics if `phase` is not valid according to the runtime's [`RuntimeConfig`].
    pub fn dispatch_reactions(&mut self, phase: &'static ReactionPhase) -> bool {
        Globals::assert_valid_reaction_phase(phase);
        self.as_raw().dispatch_reactions_with(Some(phase))
    }

    /// Dispatch scheduled reactions for all phases.
    ///
    /// Returns `true` if any reaction was dispatched.
    pub fn dispatch_all_reactions(&mut self) -> bool {
        self.as_raw().dispatch_reactions_with(None)
    }

    /// Dispatch scheduled discards.
    ///
    /// Returns `true` if any discard was dispatched.
    pub fn dispatch_discards(&mut self) -> bool {
        self.as_raw().dispatch_discards()
    }

    /// Flush all pending operations.
    ///
    /// Repeats [`dispatch_all_actions`](Self::dispatch_all_actions), [`dispatch_all_reactions`](Self::dispatch_all_reactions),
    /// and [`dispatch_discards`](Self::dispatch_discards) until there are no more pending operations.
    pub fn flush(&mut self) {
        self.as_raw().flush()
    }

    /// Lends the runtime's ownership to the current thread, making [`Runtime::call`] available during that time.
    pub fn lend(&mut self) -> RuntimeLend<'_> {
        Globals::with(|g| {
            g.runtime = self.raw.take();
        });
        RuntimeLend(self)
    }

    /// Calls a function with the runtime as an argument.
    ///
    /// # Panics
    ///
    /// Panics if no [`Runtime`] exists in the current thread.
    ///
    /// Panics if not called within [`Runtime::lend`] or if [`Runtime::call`] is reentered.
    pub fn call<T>(f: impl FnOnce(&mut Runtime) -> T) -> T {
        Self::try_call(f).unwrap_or_else(|e| panic!("{e}"))
    }

    /// Tries to call a function with the runtime as an argument.
    pub fn try_call<T>(f: impl FnOnce(&mut Runtime) -> T) -> Result<T, RuntimeCallError> {
        let raw = Globals::try_with(|g| {
            if g.runtime_config.is_none() {
                return Err(RuntimeCallError::RuntimeDoesNotExist);
            }
            g.runtime.take().ok_or(RuntimeCallError::RuntimeUnavailable)
        })
        .map_err(|_| RuntimeCallError::RuntimeDoesNotExist)??;
        Ok(f(&mut Self {
            is_owned: false,
            raw: Some(raw),
        }))
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        if self.is_owned {
            self.begin_shutdown();
            Globals::finish_runtime();
        } else {
            Globals::with(|g| {
                assert!(g.runtime.is_none());
                g.runtime = self.raw.take();
            });
        }
    }
}
pub struct RuntimeLend<'a>(&'a mut Runtime);

impl RuntimeLend<'_> {
    /// Waits for executable work or lifetime maintenance.
    ///
    /// Dormant reaction phases do not count as ready. A Closing runtime never
    /// becomes ready and does not retain a driver waker.
    pub async fn wait_for_ready(&mut self) {
        poll_fn(|cx| Globals::wait_for_ready(cx.waker())).await
    }
}

impl Drop for RuntimeLend<'_> {
    fn drop(&mut self) {
        Globals::with(|g| {
            self.0.raw = g.runtime.take();
        });
    }
}
struct RawRuntime {
    rt: RuntimeData,
    bump: Bump,
    notifys_buffer: Vec<NotifyReaction>,
    batch_buffer: Vec<(usize, usize)>,
    unbinds_buffer: Vec<SourceBindingsData>,
}
impl RawRuntime {
    pub fn ac(&mut self) -> &mut ActionContext {
        ActionContext::new(self)
    }
    fn nc(&mut self) -> &mut NotifyContext {
        self.ac().nc()
    }
    fn rc(&mut self) -> ReactionContext<'_, '_> {
        self.apply_notify();
        self.rc_raw()
    }
    fn rc_raw(&mut self) -> ReactionContext<'_, '_> {
        ReactionContext(self.sc_raw())
    }
    fn sc(&mut self) -> SignalContext<'_, '_> {
        self.apply_notify();
        self.sc_raw()
    }
    fn sc_raw(&mut self) -> SignalContext<'_, '_> {
        SignalContext {
            rt: &mut self.rt,
            bump: &self.bump,
            sink: None,
        }
    }
    fn dispatch_action(&mut self, phase: &'static ActionPhase) -> bool {
        let Some(action) = Globals::get_action(phase) else {
            return false;
        };
        action.call(self.ac());
        true
    }
    fn dispatch_actions_with(&mut self, phase: Option<&'static ActionPhase>) -> bool {
        let mut handled = false;
        let mut batch = take(&mut self.batch_buffer);
        while Globals::get_action_batch(phase, &mut batch) {
            for (index, count) in batch.drain(..) {
                for _ in 0..count {
                    let action = Globals::with(|g| g.actions[index].pop_front()).unwrap();
                    action.call(self.ac());
                    handled = true;
                }
            }
        }
        self.batch_buffer = batch;
        handled
    }

    fn dispatch_reactions_with(&mut self, phase: Option<&'static ReactionPhase>) -> bool {
        if !Globals::is_running() {
            return false;
        }
        self.apply_notify();
        let mut batch = take(&mut self.batch_buffer);
        Globals::get_reaction_batch(phase, &mut batch);
        let handled = !batch.is_empty();
        // Snapshot only counts. Unexecuted work remains in its owning queue even
        // when a callback unwinds; newly scheduled work waits for the next batch.
        for (id, count) in batch.drain(..) {
            for _ in 0..count {
                let reaction = Globals::with(|g| {
                    let reaction = g.reactions[id].pop_front().unwrap();
                    g.runnable_reactions -= 1;
                    reaction
                });
                reaction.run(&mut self.rc_raw());
            }
        }
        self.batch_buffer = batch;
        handled
    }
    fn apply_unbind(&mut self) -> bool {
        let mut handled = false;
        let mut unbinds = take(&mut self.unbinds_buffer);
        while Globals::swap_source_bindings(|g| &mut g.unbinds, &mut unbinds) {
            for unbind in unbinds.drain(..) {
                for sb in unbind {
                    sb.unbind(&mut self.rc_raw());
                }
                handled = true;
            }
        }
        self.unbinds_buffer = unbinds;
        handled
    }
    fn apply_notify(&mut self) -> bool {
        if !Globals::is_running() {
            return false;
        }
        let mut handled = self.apply_unbind();
        let mut notifys = take(&mut self.notifys_buffer);
        while Globals::get_notifys(&mut notifys) {
            for notify in notifys.drain(..) {
                notify.call_notify(self.nc());
                handled = true;
            }
        }
        self.notifys_buffer = notifys;
        handled
    }

    fn dispatch_discards(&mut self) -> bool {
        if !Globals::is_running() {
            return false;
        }
        let mut handled = false;
        loop {
            if let Some(reaction) = Globals::with(|g| g.discards.pop()) {
                reaction.run(&mut self.rc_raw());
                handled = true;
                continue;
            }
            if self.apply_unbind() {
                handled = true;
                continue;
            }
            break;
        }
        handled
    }

    fn flush(&mut self) {
        loop {
            if self.dispatch_actions_with(None) {
                continue;
            }
            if self.dispatch_reactions_with(None) {
                continue;
            }
            if self.dispatch_discards() {
                continue;
            }
            // Maintenance can enqueue Actions without executing any Reaction.
            // Check the same readiness predicate as the waiting driver before
            // declaring this runtime settled.
            if !Globals::with(|g| g.has_ready_work(&g.wakes.requests.0.lock().unwrap())) {
                break;
            }
        }
    }

    fn cancel_async_actions(&mut self) {
        let actions = take(&mut self.rt.async_actions);
        for (_, action) in actions {
            action.cancel(self.ac());
        }
    }
}

struct RuntimeData {
    async_actions: SlabMap<Rc<AsyncAction>>,
}

impl RuntimeData {
    pub fn new() -> Self {
        Self {
            async_actions: SlabMap::new(),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Debug)]
pub enum DirtyLevel {
    MaybeDirty,
    Dirty,
}
impl DirtyLevel {
    /// Returns the `DirtyLevel` accounting for state change checks that may
    /// prevent marking sinks as dirty when unchanged.
    ///
    /// * `is_check` - Whether state change checking is enabled.
    ///
    /// Returns `MaybeDirty` if `is_check` is true, otherwise returns `self`.
    pub fn maybe_if(self, is_check: bool) -> Self {
        if is_check {
            DirtyLevel::MaybeDirty
        } else {
            self
        }
    }
    pub fn is_dirty(self) -> bool {
        self == DirtyLevel::Dirty
    }
    pub fn is_maybe_dirty(self) -> bool {
        self == DirtyLevel::MaybeDirty
    }
}

impl From<DirtyLevel> for Dirty {
    fn from(value: DirtyLevel) -> Self {
        match value {
            DirtyLevel::Dirty => Dirty::Dirty,
            DirtyLevel::MaybeDirty => Dirty::MaybeDirty,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Slot(pub usize);

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct BindKey(usize);

struct SourceBinding {
    source: Rc<dyn BindSource>,
    slot: Slot,
    key: BindKey,
}
impl SourceBinding {
    fn is_same(&self, node: &Rc<dyn BindSource>, slot: Slot) -> bool {
        Rc::ptr_eq(&self.source, node) && self.slot == slot
    }
    fn check(&self, rc: &mut ReactionContext<'_, '_>) -> bool {
        self.source.clone().check(self.slot, self.key, rc)
    }
    fn unbind(self, rc: &mut ReactionContext<'_, '_>) {
        self.source.unbind(self.slot, self.key, rc);
    }
    fn rebind(self, sc: &mut SignalContext<'_, '_>) {
        self.source.rebind(self.slot, self.key, sc);
    }
}

type SourceBindingsData = Vec<SourceBinding>;

#[derive(Default)]
pub struct SourceBindings(SourceBindingsData);

impl SourceBindings {
    pub fn new() -> Self {
        Self::default()
    }
    /// Checks if any of the bound sources have been modified.
    ///
    /// Returns `true` if at least one source is dirty, `false` if all sources are clean.
    pub fn check(&self, rc: &mut ReactionContext<'_, '_>) -> bool {
        for source in &self.0 {
            if source.check(rc) {
                return true;
            }
        }
        false
    }
    fn check_with(&mut self, dirty: &mut Dirty, rc: &mut ReactionContext<'_, '_>) -> bool {
        if *dirty == Dirty::MaybeDirty {
            *dirty = Dirty::from_is_dirty(self.check(rc));
        }
        *dirty == Dirty::Dirty
    }

    /// Computes state and records dependencies on self.
    ///
    /// # Arguments
    ///
    /// * `sink` - The notification target when a dependency is updated and `update` needs to be called again.
    /// * `slot` - The notification slot.
    /// * `reset` - If `true`, clears existing dependencies. Set to `false` when building state incrementally across multiple `update` calls to preserve previous dependencies.
    /// * `f` - The function that computes the state.
    pub fn update<'r, T>(
        &mut self,
        sink: Weak<dyn BindSink>,
        slot: Slot,
        reset: bool,
        f: impl FnOnce(&mut SignalContext<'r, '_>) -> T,
        rc: &mut ReactionContext<'r, '_>,
    ) -> T {
        let mut sink = Sink::for_update(self, sink, slot, reset);
        let ret = f(&mut sink.sc(rc));
        sink.finish_update(self, rc);
        ret
    }

    /// Clears all dependencies immediately.
    ///
    /// Dropping self also clears dependencies, but since `ReactionContext` (required for editing dependencies) is not available during drop,
    /// the clearing is deferred until `ReactionContext` becomes available.
    ///
    /// Therefore, to guarantee that no further notifications will occur, use this method to clear dependencies immediately.
    ///
    /// `ReactionContext` is required for editing dependencies to prevent `BorrowMutError` in the `RefCell` that records dependencies.
    pub fn clear(&mut self, rc: &mut ReactionContext<'_, '_>) {
        for b in self.0.drain(..) {
            b.unbind(rc)
        }
    }
}
impl Drop for SourceBindings {
    fn drop(&mut self) {
        if !self.0.is_empty() {
            let _ = Globals::try_with(|g| g.push_unbinds(take(&mut self.0)));
        }
    }
}

struct SinkBinding {
    sink: Weak<dyn BindSink>,
    slot: Slot,
    dirty: Dirty,
}

impl SinkBinding {
    fn notify(&self, level: DirtyLevel, nc: &mut NotifyContext) {
        if let Some(node) = self.sink.upgrade() {
            node.notify(self.slot, level, nc)
        }
    }
}

#[derive(Default)]
pub struct SinkBindings(SlabMap<SinkBinding>);

impl SinkBindings {
    pub fn new() -> Self {
        Self(SlabMap::new())
    }
    pub fn bind(
        &mut self,
        this: Rc<dyn BindSource>,
        this_slot: Slot,
        sc: &mut SignalContext<'_, '_>,
    ) {
        let Some(sink) = &mut sc.sink else {
            return;
        };
        let sources_index = sink.sources_len;
        if let Some(source_old) = sink.sources.0.get(sources_index)
            && source_old.is_same(&this, this_slot)
        {
            sink.sources_len += 1;
            self.0[source_old.key.0].dirty = Dirty::Clean;
            return;
        }
        let key = BindKey(self.0.insert(SinkBinding {
            sink: sink.sink.clone(),
            slot: sink.slot,
            dirty: Dirty::Clean,
        }));
        if let Some(old) = sink.push(SourceBinding {
            source: this,
            slot: this_slot,
            key,
        }) {
            old.unbind(sc.rc());
        }
    }
    pub fn rebind(
        &mut self,
        this: Rc<dyn BindSource>,
        this_slot: Slot,
        key: BindKey,
        sc: &mut SignalContext<'_, '_>,
    ) {
        if let Some(sink) = &mut sc.sink {
            self.0[key.0].slot = sink.slot;
            if let Some(old) = sink.push(SourceBinding {
                source: this,
                slot: this_slot,
                key,
            }) {
                old.unbind(sc.rc());
            }
        } else {
            self.unbind(key, sc.rc());
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn is_dirty(&self, key: BindKey, _uc: &mut ReactionContext<'_, '_>) -> bool {
        match self.0[key.0].dirty {
            Dirty::Clean => false,
            Dirty::MaybeDirty => panic!("`is_dirty` called before `update()`"),
            Dirty::Dirty => true,
        }
    }
    /// Unbinds the dependency identified by the given `key`.
    pub fn unbind(&mut self, key: BindKey, _uc: &mut ReactionContext<'_, '_>) {
        self.0.remove(key.0);
    }

    pub fn notify(&mut self, level: DirtyLevel, nc: &mut NotifyContext) {
        self.0.optimize();
        for binding in self.0.values_mut() {
            if binding.dirty.needs_notify() {
                binding.notify(level, nc);
            }
            binding.dirty.apply_notify(level);
        }
    }
    pub fn update(&mut self, is_dirty: bool, _uc: &mut ReactionContext<'_, '_>) {
        self.0.optimize();
        for binding in self.0.values_mut() {
            if binding.dirty == Dirty::MaybeDirty {
                binding.dirty = Dirty::from_is_dirty(is_dirty);
            }
        }
    }
}

struct Sink {
    sink: Weak<dyn BindSink>,
    slot: Slot,
    sources: SourceBindings,
    sources_len: usize,
}
impl Sink {
    fn for_update(
        sources: &mut SourceBindings,
        sink: Weak<dyn BindSink>,
        slot: Slot,
        reset: bool,
    ) -> Self {
        Self {
            sink,
            slot,
            sources_len: if reset { 0 } else { sources.0.len() },
            sources: take(sources),
        }
    }

    fn sc<'r, 's>(&'s mut self, rc: &'s mut ReactionContext<'r, '_>) -> SignalContext<'r, 's> {
        SignalContext {
            rt: rc.0.rt,
            bump: rc.0.bump,
            sink: Some(self),
        }
    }

    fn finish_update(&mut self, sources: &mut SourceBindings, rc: &mut ReactionContext<'_, '_>) {
        // Only a successful callback restores dependencies. On unwind, dropping
        // this sink defers all unbinds, including dependencies added by the callback.
        *sources = take(&mut self.sources);
        for binding in sources.0.drain(self.sources_len..) {
            binding.unbind(rc);
        }
    }

    #[must_use]
    fn push(&mut self, binding: SourceBinding) -> Option<SourceBinding> {
        let index = self.sources_len;
        self.sources_len += 1;
        if index < self.sources.0.len() {
            Some(replace(&mut self.sources.0[index], binding))
        } else {
            self.sources.0.push(binding);
            None
        }
    }
}

/// Context required to read state in operations that do not modify state.
#[repr(transparent)]
pub struct ReactionContext<'r, 's>(SignalContext<'r, 's>);

impl<'r, 's> ReactionContext<'r, 's> {
    pub(crate) fn can_poll(&self) -> bool {
        Globals::is_running()
    }

    fn new<'a>(sc: &'a mut SignalContext<'r, 's>) -> &'a mut Self {
        unsafe { transmute(sc) }
    }

    /// Register a Reaction to discard the cache.
    ///
    /// Registered reactions are called when [`Runtime::dispatch_discards`] is called.
    pub fn schedule_discard(&mut self, discard: Reaction) {
        Globals::with(|g| {
            g.discards.push(discard);
            g.wake();
        })
    }

    /// Call a function with a [`SignalContext`] that does not track dependencies.
    pub fn sc_with<T>(&mut self, f: impl FnOnce(&mut SignalContext<'r, 's>) -> T) -> T {
        self.0.untrack(f)
    }

    /// Borrow a [`RefCell`] that succeeds in borrowing if there are no cyclic dependencies.
    pub fn borrow<'a, T>(&self, cell: &'a RefCell<T>) -> Ref<'a, T> {
        match cell.try_borrow() {
            Ok(b) => b,
            Err(_) => panic!("detect cyclic dependency"),
        }
    }
}

/// Context for state invalidation notification
#[repr(transparent)]
pub struct NotifyContext(ActionContext);

impl NotifyContext {
    fn new(ac: &mut ActionContext) -> &mut Self {
        unsafe { transmute(ac) }
    }
}

/// Schedules state invalidation notifications.
///
/// If [`NotifyContext`] is available, this function should not be called and update notification should be done directly.
pub fn schedule_notify(node: Weak<dyn BindSink>, slot: Slot) {
    let _ = Globals::try_with(|rg| rg.push_notify(node, slot));
}

/// Context for retrieving state and tracking dependencies.
pub struct SignalContext<'r, 's> {
    rt: &'s mut RuntimeData,
    bump: &'r Bump,
    sink: Option<&'s mut Sink>,
}

impl<'r, 's> SignalContext<'r, 's> {
    pub fn rc(&mut self) -> &mut ReactionContext<'r, 's> {
        ReactionContext::new(self)
    }

    /// Call a function with a [`SignalContext`] that does not track dependencies.
    pub fn untrack<T>(&mut self, f: impl FnOnce(&mut SignalContext<'r, 's>) -> T) -> T {
        struct UntrackGuard<'r, 's, 'a> {
            sc: &'a mut SignalContext<'r, 's>,
            sink: Option<&'s mut Sink>,
        }
        impl<'r, 's> Drop for UntrackGuard<'r, 's, '_> {
            fn drop(&mut self) {
                self.sc.sink = self.sink.take();
            }
        }
        f(UntrackGuard {
            sink: self.sink.take(),
            sc: self,
        }
        .sc)
    }
    fn extend(&mut self, from: &mut SourceBindings) {
        for binding in from.0.drain(..) {
            binding.rebind(self);
        }
    }
}

/// A trait for types that can be notified of state changes.
pub trait BindSink: 'static {
    fn notify(self: Rc<Self>, slot: Slot, level: DirtyLevel, nc: &mut NotifyContext);
}

/// A trait for types that can hold a state and be monitored for changes.
pub trait BindSource: 'static {
    /// Checks if this source has been modified since the last check.
    ///
    /// Returns `true` if the source is dirty (has changes), `false` if clean (no changes).
    fn check(self: Rc<Self>, slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) -> bool;
    fn unbind(self: Rc<Self>, slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>);
    fn rebind(self: Rc<Self>, slot: Slot, key: BindKey, sc: &mut SignalContext<'_, '_>);
}

#[derive(Clone)]
struct NotifyReaction {
    sink: Weak<dyn BindSink>,
    slot: Slot,
}
impl NotifyReaction {
    fn call_notify(&self, nc: &mut NotifyContext) {
        if let Some(sink) = self.sink.upgrade() {
            sink.notify(self.slot, DirtyLevel::Dirty, nc)
        }
    }
}

/// Context required for operations that modify state.
#[repr(transparent)]
pub struct ActionContext(RawRuntime);

impl ActionContext {
    fn new(rt: &mut RawRuntime) -> &mut Self {
        unsafe { transmute(rt) }
    }
    pub fn nc(&mut self) -> &mut NotifyContext {
        NotifyContext::new(self)
    }
    pub fn rc(&mut self) -> ReactionContext<'_, '_> {
        self.0.rc()
    }
    pub fn sc(&mut self) -> SignalContext<'_, '_> {
        self.0.sc()
    }

    /// Starts an asynchronous action inline, resuming in the default phase.
    ///
    /// Polls the handler once using this context before returning. An immediately ready
    /// `.await` does not suspend execution. If the handler completes, its future is dropped
    /// before this method returns. Otherwise, the runtime owns the future and resumes it
    /// after a wake in the default [`ActionPhase`], dropping it when the runtime shuts down.
    /// No other queued actions are dispatched by this method. In Closing the
    /// handler is cancelled without construction or polling.
    ///
    /// The handler owns its [`AsyncActionContext`]. Use [`AsyncActionContext::call`] to
    /// modify state in synchronous sections, and [`AsyncActionContext::is_running_inline`]
    /// to check whether execution is still inside this method's initial poll.
    ///
    /// # Panics
    ///
    /// Panics before invoking the handler if Running and the default [`ActionPhase`] is not valid
    /// according to the runtime's [`RuntimeConfig`].
    ///
    /// ```
    /// use sigmut::{State, core::Runtime};
    ///
    /// let mut runtime = Runtime::new();
    /// let state = State::new(0);
    /// runtime.ac().start_async_inline({
    ///     let state = state.clone();
    ///     async move |aac| {
    ///         let value = std::future::ready(42).await;
    ///         assert!(aac.is_running_inline());
    ///         aac.call(|ac| state.set(value, ac));
    ///     }
    /// });
    /// assert_eq!(state.get(&mut runtime.sc()), 42);
    /// ```
    pub fn start_async_inline(&mut self, f: impl AsyncFnOnce(AsyncActionContext) + 'static) {
        self.start_async_inline_in(ActionPhase::DEFAULT, f);
    }

    /// Starts an asynchronous action inline, resuming in the specified phase.
    ///
    /// Like [`start_async_inline`](Self::start_async_inline), the first poll runs
    /// synchronously using this context. `resume_phase` only controls continuation after
    /// the handler returns [`Poll::Pending`]; the first poll does not wait for that phase.
    /// Even if the handler wakes itself, further polls are left to the runtime.
    ///
    /// # Panics
    ///
    /// Panics before invoking the handler if Running and `resume_phase` is not valid according to
    /// the runtime's [`RuntimeConfig`].
    pub fn start_async_inline_in(
        &mut self,
        resume_phase: &'static ActionPhase,
        f: impl AsyncFnOnce(AsyncActionContext) + 'static,
    ) {
        AsyncAction::start(resume_phase, self, true, |aac| async move { f(aac).await });
    }
}

/// Spawns a new action.
///
/// # Panics
///
/// Panics if no [`Runtime`] exists, or if Running and the default phase is unsupported.
pub fn spawn_action(f: impl FnOnce(&mut ActionContext) + 'static) {
    Action::new(f).schedule()
}

/// Spawns a new action with a specific phase.
///
/// # Panics
///
/// Panics if no [`Runtime`] exists, or if Running and `phase` is unsupported.
pub fn spawn_action_in(phase: &'static ActionPhase, f: impl FnOnce(&mut ActionContext) + 'static) {
    Action::new(f).schedule_in(phase)
}

/// Spawns a new asynchronous action.
///
/// The handler starts when its scheduled action is dispatched, and always observes
/// [`AsyncActionContext::is_running_inline`] as `false`. To poll a handler before returning,
/// use [`ActionContext::start_async_inline`].
///
/// # Panics
///
/// Panics if no [`Runtime`] exists, or if Running and the default phase is unsupported.
pub fn spawn_action_async(f: impl AsyncFnOnce(&mut AsyncActionContext) + 'static) {
    spawn_action_async_in(ActionPhase::DEFAULT, f)
}

/// Spawns a new asynchronous action with a specific phase.
///
/// Both the initial poll and wake-driven continuations are scheduled in `phase`.
/// The handler always observes [`AsyncActionContext::is_running_inline`] as `false`.
/// To run the first poll immediately, use [`ActionContext::start_async_inline_in`].
///
/// # Panics
///
/// Panics if no [`Runtime`] exists, or if Running and `phase` is unsupported.
pub fn spawn_action_async_in(
    phase: &'static ActionPhase,
    f: impl AsyncFnOnce(&mut AsyncActionContext) + 'static,
) {
    spawn_action_in(phase, move |ac| {
        AsyncAction::start(phase, ac, false, |mut ac| async move {
            f(&mut ac).await;
        })
    })
}

/// A recipe for state mutation, registered by [`schedule`](Self::schedule).
///
/// Construction does not need a runtime. Scheduling requires a runtime and
/// transfers ownership to its session. Closing cancels the recipe without running
/// it, releasing captures; captures may queue persistent Reactions on cancellation.
/// Scheduling returns no completion handle; AsyncActions likewise return `()`.
pub struct Action(RawAction);

#[allow(clippy::type_complexity)]
enum RawAction {
    Box(Box<dyn FnOnce(&mut ActionContext)>),
    Rc {
        this: Rc<dyn Any>,
        f: Box<dyn Fn(Rc<dyn Any>, &mut ActionContext)>,
    },
    Weak {
        this: Weak<dyn Any>,
        f: Box<dyn Fn(Weak<dyn Any>, &mut ActionContext)>,
    },
}

impl Action {
    /// Creates a new action from a boxed closure.
    pub fn new(f: impl FnOnce(&mut ActionContext) + 'static) -> Self {
        Action(RawAction::Box(downcast_or(f, |f| Box::new(f))))
    }

    /// Creates a new action from an Rc without heap allocation.
    ///
    /// `f` should be of a zero-sized type.
    /// If `f` is not a zero-sized type, heap allocation will occur.
    pub fn from_rc_fn<T: Any>(
        this: Rc<T>,
        f: impl Fn(Rc<T>, &mut ActionContext) + Copy + 'static,
    ) -> Self {
        Action(RawAction::Rc {
            this,
            f: Box::new(move |this, ac| f(this.downcast().unwrap(), ac)),
        })
    }

    /// Creates a new action from a weak reference.
    ///
    /// `f` should be of a zero-sized type.
    /// If `f` is not a zero-sized type, heap allocation will occur.
    pub fn from_weak_fn<T: Any>(
        this: Weak<T>,
        f: impl Fn(Rc<T>, &mut ActionContext) + Copy + 'static,
    ) -> Self {
        Action(RawAction::Weak {
            this,
            f: Box::new(move |this, ac| {
                if let Some(this) = this.upgrade() {
                    f(this.downcast().unwrap(), ac)
                }
            }),
        })
    }

    /// Schedules this action with a specific phase.
    ///
    /// # Panics
    ///
    /// Panics if no [`Runtime`] exists, or if Running and `phase` is unsupported.
    pub fn schedule_in(self, phase: &'static ActionPhase) {
        Globals::schedule_action(phase, self)
    }

    /// Schedules this action with default phase.
    ///
    /// # Panics
    ///
    /// Panics if no [`Runtime`] exists, or if Running and the default phase is unsupported.
    pub fn schedule(self) {
        self.schedule_in(ActionPhase::DEFAULT)
    }

    fn call(self, ac: &mut ActionContext) {
        match self.0 {
            RawAction::Box(f) => f(ac),
            RawAction::Rc { this, f } => f(this, ac),
            RawAction::Weak { this, f } => f(this, ac),
        }
    }
}
struct AsyncAction {
    aac_source: AsyncActionContextSource,
    data: RefCell<Option<AsyncActionData>>,
}
impl AsyncAction {
    fn start<Fut>(
        phase: &'static ActionPhase,
        ac: &mut ActionContext,
        inline: bool,
        f: impl FnOnce(AsyncActionContext) -> Fut + 'static,
    ) where
        Fut: Future<Output = ()> + 'static,
    {
        let Some(index) = Globals::with(|g| g.action_phase_index(phase)) else {
            return;
        };
        let aac_source = AsyncActionContextSource::new();
        let aac = aac_source.context();
        let future = aac_source.call(ac, || f(aac));
        let action = Rc::new(Self {
            aac_source,
            data: RefCell::new(None),
        });
        let id = ac.0.rt.async_actions.insert(action.clone());
        let wake = WakeReaction::AsyncAction {
            action: Rc::downgrade(&action),
            index,
        }
        .into_raw_wake();
        *action.data.borrow_mut() = Some(AsyncActionData {
            id,
            waker: wake.clone().into(),
            wake,
            future: Box::pin(future),
        });
        action.poll(ac, inline);
    }
    fn cancel(self: &Rc<Self>, ac: &mut ActionContext) {
        let data = self.data.borrow_mut().take();
        if let Some(data) = data {
            self.aac_source.call(ac, || drop(data));
        }
    }
    fn next(self: Rc<Self>, ac: &mut ActionContext) {
        self.poll(ac, false);
    }
    fn poll(self: Rc<Self>, ac: &mut ActionContext, inline: bool) {
        let Some(id) = self.data.borrow().as_ref().map(|data| data.id) else {
            return;
        };
        // The executing future owns itself. Unwind must not leave an empty task
        // registered forever, and its destructor still needs the original context.
        ac.0.rt.async_actions.remove(id);
        let pending = self.aac_source.call(ac, || {
            let mut data = self.data.borrow_mut().take().unwrap();
            let result = self.aac_source.with_running_inline(inline, || {
                data.future
                    .as_mut()
                    .poll(&mut Context::from_waker(&data.waker))
            });
            if result.is_ready() {
                drop(data);
                false
            } else {
                *self.data.borrow_mut() = Some(data);
                true
            }
        });
        if pending {
            let id = ac.0.rt.async_actions.insert(self.clone());
            self.data.borrow_mut().as_mut().unwrap().id = id;
        }
    }
}

struct AsyncActionData {
    future: Pin<Box<dyn Future<Output = ()>>>,
    waker: Waker,
    wake: Arc<RawWake>,
    id: usize,
}
impl Drop for AsyncActionData {
    fn drop(&mut self) {
        // Invalidate queued requests too: the slab slot may be reused while an
        // old waker is still held externally. No old wake can target its successor.
        self.wake.active.store(false, Ordering::Relaxed);
        let _ = Globals::try_with(|g| g.wakes.reactions.remove(self.wake.key));
    }
}

#[derive(Default)]
struct WakeTable {
    reactions: SlabMap<WakeReaction>,
    requests: WakeRequests,
}

impl WakeTable {
    fn insert(&mut self, reaction: WakeReaction) -> Arc<RawWake> {
        RawWake::new(&self.requests, self.reactions.insert(reaction))
    }
}
enum WakeReaction {
    Notify(NotifyReaction),
    AsyncAction {
        action: Weak<AsyncAction>,
        // Cancellation invalidates this wake before another session can reuse the index.
        index: usize,
    },
}
impl WakeReaction {
    fn into_raw_wake(self) -> Arc<RawWake> {
        Globals::with(|sc| sc.wakes.insert(self))
    }
    fn into_waker(self) -> Waker {
        self.into_raw_wake().into()
    }
}
pub fn waker_from_sink(sink: Weak<impl BindSink>, slot: Slot) -> Waker {
    WakeReaction::Notify(NotifyReaction { sink, slot }).into_waker()
}

#[derive(Clone, Default)]
struct WakeRequests(Arc<Mutex<RawWakeRequests>>);

#[derive(Default)]
struct RawWakeRequests {
    wakes: Vec<(usize, Arc<AtomicBool>)>,
    drops: Vec<usize>,
    waker: Option<Waker>,
}
struct RawWake {
    requests: WakeRequests,
    key: usize,
    active: Arc<AtomicBool>,
}
impl RawWake {
    fn new(requests: &WakeRequests, key: usize) -> Arc<Self> {
        Arc::new(RawWake {
            requests: requests.clone(),
            key,
            active: Arc::new(AtomicBool::new(true)),
        })
    }
}

impl Wake for RawWake {
    fn wake(self: Arc<Self>) {
        let waker = {
            let mut requests = self.requests.0.lock().unwrap();
            if !self.active.load(Ordering::Relaxed) {
                return;
            }
            requests.wakes.push((self.key, self.active.clone()));
            requests.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}
impl Drop for RawWake {
    fn drop(&mut self) {
        if !self.active.load(Ordering::Relaxed) {
            return;
        }
        let waker = {
            let mut requests = self.requests.0.lock().unwrap();
            requests.drops.push(self.key);
            requests.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

struct AsyncActionContextData {
    channel: ActionContextChannel,
    is_running_inline: Cell<bool>,
}

struct AsyncActionContextSource(Rc<AsyncActionContextData>);

impl AsyncActionContextSource {
    fn new() -> Self {
        Self(Rc::new(AsyncActionContextData {
            channel: ActionContextChannel::new(),
            is_running_inline: Cell::new(false),
        }))
    }
    fn call<T>(&self, ac: &mut ActionContext, f: impl FnOnce() -> T) -> T {
        self.0.channel.scope(ac, f)
    }
    fn with_running_inline<T>(&self, inline: bool, f: impl FnOnce() -> T) -> T {
        struct ResetInline<'a>(&'a Cell<bool>);

        impl Drop for ResetInline<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }

        self.0.is_running_inline.set(inline);
        let _reset = ResetInline(&self.0.is_running_inline);
        f()
    }
    fn context(&self) -> AsyncActionContext {
        AsyncActionContext(self.0.clone())
    }
}

/// Context for asynchronous state change.
pub struct AsyncActionContext(Rc<AsyncActionContextData>);

impl AsyncActionContext {
    /// Returns whether this action is currently running inline.
    ///
    /// Returns `true` during the initial synchronous poll performed by
    /// [`ActionContext::start_async_inline`] or [`ActionContext::start_async_inline_in`].
    /// Immediately ready `.await` expressions keep this value `true`. It is `false` after
    /// that poll returns or unwinds, including during later polls following a suspension.
    /// Actions started by [`spawn_action_async`] or [`spawn_action_async_in`] always
    /// observe `false`, even in their first poll.
    ///
    /// This describes the current execution, not how the action was originally started.
    /// "Inline" refers to execution within the starting method's call, not compiler inlining.
    pub fn is_running_inline(&self) -> bool {
        self.0.is_running_inline.get()
    }

    /// Uses this Action's context during its poll or cancellation destructor.
    ///
    /// # Panics
    /// Panics outside that synchronous scope, including after the session ends.
    /// A later Runtime never reactivates this context.
    pub fn call<T>(&self, f: impl FnOnce(&mut ActionContext) -> T) -> T {
        self.0
            .channel
            .try_with(f)
            .expect("`AsyncActionContext` cannot be used after being moved.")
    }
}

/// Operations that do not modify state.
pub struct Reaction(RawReaction);

pub(crate) trait DiscardReaction: 'static {
    fn discard(self: Rc<Self>, rc: &mut ReactionContext<'_, '_>);
}

impl Reaction {
    pub(crate) fn from_discard(this: Rc<dyn DiscardReaction>) -> Self {
        Reaction(RawReaction::Discard(this))
    }

    pub fn new(f: impl FnOnce(&mut ReactionContext<'_, '_>) + 'static) -> Self {
        Reaction(RawReaction::Box(downcast_or(f, |f| Box::new(f))))
    }

    /// Creates a new Reaction from an Rc without heap allocation.
    ///
    /// `f` should be of a zero-sized type.
    /// If `f` is not a zero-sized type, heap allocation will occur.
    pub fn from_rc_fn<T: Any>(
        this: Rc<T>,
        f: impl Fn(Rc<T>, &mut ReactionContext<'_, '_>) + Copy + 'static,
    ) -> Self {
        Reaction(RawReaction::Rc {
            this,
            f: Box::new(move |this, rc| f(this.downcast().unwrap(), rc)),
        })
    }

    /// Creates a new Reaction from a weak reference.
    ///
    /// `f` should be of a zero-sized type.
    /// If `f` is not a zero-sized type, heap allocation will occur.
    pub fn from_weak_fn<T: Any>(
        this: Weak<T>,
        f: impl Fn(Rc<T>, &mut ReactionContext<'_, '_>) + Copy + 'static,
    ) -> Self {
        Reaction(RawReaction::Weak {
            this,
            f: Box::new(move |this, rc| {
                if let Some(this) = this.upgrade() {
                    f(this.downcast().unwrap(), rc)
                }
            }),
        })
    }

    /// Queues this reaction persistently in the specified phase.
    ///
    /// Accepted without a Runtime, during Closing, and for unsupported phases.
    /// A matching Running runtime dispatches it in phase-local FIFO order. An
    /// arbitrary closure retains its captures until dispatch or thread exit;
    /// owner-bound work should use [`from_weak_fn`](Self::from_weak_fn).
    pub fn schedule_in(self, phase: &'static ReactionPhase) {
        Globals::schedule_reaction(phase, self)
    }
    /// Queues this reaction in the shared default phase.
    pub fn schedule(self) {
        self.schedule_in(ReactionPhase::DEFAULT);
    }
    fn run(self, rc: &mut ReactionContext<'_, '_>) {
        match self.0 {
            RawReaction::Box(f) => f(rc),
            RawReaction::Discard(this) => this.discard(rc),
            RawReaction::Rc { this, f } => f(this, rc),
            RawReaction::Weak { this, f } => f(this, rc),
        }
    }
}

enum RawReaction {
    Discard(Rc<dyn DiscardReaction>),
    Box(Box<dyn FnOnce(&mut ReactionContext<'_, '_>)>),
    Rc {
        this: Rc<dyn Any>,
        #[allow(clippy::type_complexity)]
        f: Box<dyn Fn(Rc<dyn Any>, &mut ReactionContext<'_, '_>)>,
    },
    Weak {
        this: Weak<dyn Any>,
        #[allow(clippy::type_complexity)]
        f: Box<dyn Fn(Weak<dyn Any>, &mut ReactionContext<'_, '_>)>,
    },
}

#[cfg(test)]
mod tests;
