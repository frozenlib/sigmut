use core::panic;
use std::{
    any::Any,
    cell::{Ref, RefCell},
    future::{Future, poll_fn},
    mem::{replace, swap, take, transmute},
    ops::AsyncFnOnce,
    pin::Pin,
    ptr::null_mut,
    rc::{Rc, Weak},
    result::Result,
    sync::{Arc, Mutex, MutexGuard},
    task::{Context, Poll, Wake, Waker},
    thread::AccessError,
};

use bumpalo::Bump;
use derive_ex::Ex;
use parse_display::Display;
use slabmap::SlabMap;

mod async_signal_context;
mod dirty;
mod source_binder;
mod state_ref;
mod state_ref_builder;

pub use async_signal_context::*;
pub use dirty::*;
pub use source_binder::SourceBinder;
pub use state_ref::StateRef;
pub use state_ref_builder::StateRefBuilder;

use crate::utils::buckets::Buckets;

thread_local! {
    static GLOBALS: RefCell<Globals> = RefCell::new(Globals::new());
}

struct Globals {
    is_runtime_exists: bool,
    runtime: Option<Box<RawRuntime>>,
    unbinds: Vec<SourceBindingsData>,
    actions: Buckets<Action>,
    notifys: Vec<NotifyReaction>,
    need_wake: bool,
    wakes: WakeTable,
    reactions: Buckets<Reaction>,
}
impl Globals {
    fn new() -> Self {
        let mut reactions = Buckets::new();
        let mut actions = Buckets::new();
        reactions.register_bucket(0);
        actions.register_bucket(0);
        Self {
            is_runtime_exists: false,
            runtime: None,
            unbinds: Vec::new(),
            actions,
            notifys: Vec::new(),
            need_wake: false,
            wakes: WakeTable::default(),
            reactions,
        }
    }
    fn with<T>(f: impl FnOnce(&mut Self) -> T) -> T {
        GLOBALS.with(|g| f(&mut g.borrow_mut()))
    }
    fn try_with<T>(f: impl FnOnce(&mut Self) -> T) -> Result<T, AccessError> {
        GLOBALS.try_with(|g| f(&mut g.borrow_mut()))
    }
    fn schedule_reaction(kind: ReactionKind, reaction: Reaction) {
        Self::with(|g| {
            if !g.reactions.try_push(kind.id as isize, reaction) {
                panic!("`ReactionKind` {} is not registered.", kind);
            }
            g.wake();
        })
    }

    fn schedule_action(kind: ActionKind, action: Action) {
        Self::with(|g| {
            if !g.actions.try_push(kind.id as isize, action) {
                panic!("`ActionKind` {} is not registered.", kind);
            }
            g.wake();
        })
    }
    fn get_notifys(notifys: &mut Vec<NotifyReaction>) -> bool {
        Self::with(|g| {
            g.apply_wake();
            swap(notifys, &mut g.notifys);
        });
        !notifys.is_empty()
    }

    fn get_reactions(kind: Option<ReactionKind>, reactions: &mut Vec<Reaction>) {
        Self::with(|g| {
            g.reactions.drain(kind.map(|k| k.id as isize), reactions);
        })
    }
    fn get_actions(kind: Option<ActionKind>, actions: &mut Vec<Action>) -> bool {
        Self::with(|g| {
            g.apply_wake();
            let was_empty = g.actions.is_empty();
            g.actions.drain(kind.map(|k| k.id as isize), actions);
            !was_empty
        })
    }

    fn swap_source_bindings(
        f: impl FnOnce(&mut Self) -> &mut Vec<SourceBindingsData>,
        values: &mut Vec<SourceBindingsData>,
    ) -> bool {
        Self::with(|g| swap(f(g), values));
        !values.is_empty()
    }

    fn push_notify(&mut self, sink: Weak<dyn BindSink>, slot: Slot) {
        self.notifys.push(NotifyReaction { sink, slot });
        self.wake();
    }
    fn apply_wake(&mut self) {
        let mut requests = self.wakes.requests.0.lock().unwrap();
        for key in requests.drops.drain(..) {
            self.wakes.reactions.remove(key);
        }
        for key in requests.wakes.drain(..) {
            if let Some(reaction) = self.wakes.reactions.get(key) {
                match reaction {
                    WakeReaction::Notify(reaction) => {
                        self.notifys.push(reaction.clone());
                    }
                    WakeReaction::AsyncAction(action) => {
                        let pushed = self
                            .actions
                            .try_push(action.kind.id as isize, action.to_action());
                        debug_assert!(pushed);
                    }
                }
            }
        }
    }
    fn wait_for_ready(&mut self, cx: &Context) -> Poll<()> {
        self.need_wake = false;
        if !self.notifys.is_empty()
            || !self.actions.is_empty()
            || !self.reactions.is_empty()
            || !self.unbinds.is_empty()
        {
            return Poll::Ready(());
        }
        let mut requests = self.wakes.requests.0.lock().unwrap();
        if !requests.drops.is_empty() || !requests.wakes.is_empty() {
            return Poll::Ready(());
        }
        requests.waker = Some(cx.waker().clone());
        self.need_wake = true;
        Poll::Pending
    }

    fn finish_runtime(&mut self) {
        self.is_runtime_exists = false;
        self.reactions = Buckets::new();
        self.actions = Buckets::new();
        self.reactions.register_bucket(0);
        self.actions.register_bucket(0);
    }

    fn wake(&mut self) {
        if !self.need_wake {
            return;
        }
        self.need_wake = false;
        self.wakes.requests.0.lock().unwrap().wake();
    }
    fn assert_exists(&self) {
        if !self.is_runtime_exists {
            panic!("`Runtime` is not created.");
        }
    }

    fn register_reaction_kind(&mut self, kind: ReactionKind) {
        self.assert_exists();
        self.reactions.register_bucket(kind.id as isize);
    }
    fn register_action_kind(&mut self, kind: ActionKind) {
        self.assert_exists();
        self.actions.register_bucket(kind.id as isize);
    }
    fn is_reaction_kind_registered(&self, kind: ReactionKind) -> bool {
        self.reactions.contains_bucket(kind.id as isize)
    }
    fn is_action_kind_registered(&self, kind: ActionKind) -> bool {
        self.actions.contains_bucket(kind.id as isize)
    }
}

/// Reactive runtime.
#[derive(Ex)]
#[derive_ex(Default)]
#[default(Self::new())]
pub struct Runtime {
    is_owned: bool,
    raw: Option<Box<RawRuntime>>,
}
impl Runtime {
    pub fn new() -> Self {
        if Globals::with(|g| replace(&mut g.is_runtime_exists, true)) {
            panic!("Only one `Runtime` can exist in the same thread at the same time.");
        };
        Self {
            is_owned: true,
            raw: Some(Box::new(RawRuntime {
                rt: RuntimeData::new(),
                bump: Bump::new(),
                notifys_buffer: Vec::new(),
                actions_buffer: Vec::new(),
                reactions_buffer: Vec::new(),
                unbinds_buffer: Vec::new(),
            })),
        }
    }

    fn as_raw(&mut self) -> &mut RawRuntime {
        self.raw
            .as_mut()
            .expect("Runtime is unavailable. `Runtime::wait_for_ready` may have leaked.")
    }

    pub fn register_action_kind(kind: ActionKind) {
        Globals::with(|g| g.register_action_kind(kind))
    }
    pub fn register_reaction_kind(kind: ReactionKind) {
        Globals::with(|g| g.register_reaction_kind(kind))
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

    /// Dispatch scheduled actions for the specified kind.
    ///
    /// Returns `true` if any action was dispatched.
    pub fn dispatch_actions(&mut self, kind: ActionKind) -> bool {
        self.as_raw().dispatch_actions_with(Some(kind))
    }

    /// Dispatch scheduled actions for all kinds.
    ///
    /// Returns `true` if any action was dispatched.
    pub fn dispatch_all_actions(&mut self) -> bool {
        self.as_raw().dispatch_actions_with(None)
    }

    /// Dispatch scheduled reactions for the specified kind.
    ///
    /// Returns `true` if any reaction was dispatched.
    pub fn dispatch_reactions(&mut self, kind: ReactionKind) -> bool {
        self.as_raw().dispatch_reactions_with(Some(kind))
    }

    /// Dispatch scheduled reactions for all kinds.
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
    /// Panics if not called within [`Runtime::lend`] or if [`Runtime::call`] is reentered.
    pub fn call<T>(f: impl FnOnce(&mut Runtime) -> T) -> T {
        let raw = Globals::with(|g| {
            assert!(g.is_runtime_exists, "Runtime does not exist");
            let Some(raw) = g.runtime.take() else {
                panic!("Runtime is not available. Ensure you are within a `Runtime::lend` call.");
            };
            raw
        });
        f(&mut Self {
            is_owned: false,
            raw: Some(raw),
        })
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        if self.is_owned {
            self.as_raw().cancel_async_actions();
            Globals::with(|g| g.finish_runtime());
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
    /// Wait while there is no process to be executed by [`Runtime::flush`].
    pub async fn wait_for_ready(&mut self) {
        poll_fn(|cx| Globals::with(|g| g.wait_for_ready(cx))).await
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
    actions_buffer: Vec<Action>,
    reactions_buffer: Vec<Reaction>,
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
    fn dispatch_actions_with(&mut self, kind: Option<ActionKind>) -> bool {
        let mut handled = false;
        let mut actions = take(&mut self.actions_buffer);
        while Globals::get_actions(kind, &mut actions) {
            for action in actions.drain(..) {
                action.call(self.ac());
                handled = true;
            }
        }
        self.actions_buffer = actions;
        handled
    }

    fn dispatch_reactions_with(&mut self, kind: Option<ReactionKind>) -> bool {
        self.apply_notify();
        let mut reactions = take(&mut self.reactions_buffer);
        Globals::get_reactions(kind, &mut reactions);
        let handled = !reactions.is_empty();
        for reaction in reactions.drain(..) {
            reaction.run(&mut self.rc_raw());
        }
        self.reactions_buffer = reactions;
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
        let mut handled = false;
        loop {
            if let Some(reaction) = self.rt.discards.pop() {
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
            break;
        }
    }

    fn cancel_async_actions(&mut self) {
        let mut acts = Vec::new();
        while !self.rt.async_actions.is_empty() {
            acts.extend(self.rt.async_actions.values().cloned());
            for act in &acts {
                act.cancel(self.ac());
            }
            acts.clear();
        }
    }
}

impl Drop for RawRuntime {
    fn drop(&mut self) {
        self.cancel_async_actions();
        Globals::with(|g| g.finish_runtime());
    }
}

struct RuntimeData {
    discards: Vec<Reaction>,
    async_actions: SlabMap<Rc<AsyncAction>>,
}

impl RuntimeData {
    pub fn new() -> Self {
        Self {
            discards: Vec::new(),
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
        let sources_len = if reset { 0 } else { self.0.len() };
        let mut sink = Sink {
            sink,
            slot,
            sources: take(self),
            sources_len,
        };
        let mut sc = SignalContext {
            rt: rc.0.rt,
            bump: rc.0.bump,
            sink: Some(&mut sink),
        };

        let ret = f(&mut sc);
        *self = sink.sources;
        for b in self.0.drain(sink.sources_len..) {
            b.unbind(rc);
        }
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
            let _ = Globals::try_with(|g| g.unbinds.push(take(&mut self.0)));
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
    fn new<'a>(sc: &'a mut SignalContext<'r, 's>) -> &'a mut Self {
        unsafe { transmute(sc) }
    }

    /// Register a Reaction to discard the cache.
    ///
    /// Registered reactions are called when [`Runtime::dispatch_discards`] is called.
    pub fn schedule_discard(&mut self, discard: Reaction) {
        self.0.rt.discards.push(discard)
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
}

/// Spawns a new action.
pub fn spawn_action(f: impl FnOnce(&mut ActionContext) + 'static) {
    Action::new(f).schedule()
}

/// Spawns a new action with a specific kind.
pub fn spawn_action_with(kind: ActionKind, f: impl FnOnce(&mut ActionContext) + 'static) {
    Action::new(f).schedule_with(kind)
}

/// Spawns a new asynchronous action.
pub fn spawn_action_async(f: impl AsyncFnOnce(&mut AsyncActionContext) + 'static) {
    spawn_action_async_with(ActionKind::default(), f)
}

/// Spawns a new asynchronous action with a specific kind.
pub fn spawn_action_async_with(
    kind: ActionKind,
    f: impl AsyncFnOnce(&mut AsyncActionContext) + 'static,
) {
    spawn_action_with(kind, move |ac| {
        AsyncAction::start(kind, ac, |mut ac| async move {
            f(&mut ac).await;
        })
    })
}

/// Operations that modify state.
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
        Action(RawAction::Box(Box::new(f)))
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

    /// Schedules this action with a specific kind.
    pub fn schedule_with(self, kind: ActionKind) {
        Globals::schedule_action(kind, self)
    }

    /// Schedules this action with default kind.
    pub fn schedule(self) {
        self.schedule_with(ActionKind::default())
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
    kind: ActionKind,
    aac_source: AsyncActionContextSource,
    data: RefCell<Option<AsyncActionData>>,
}
impl AsyncAction {
    fn start<Fut>(
        kind: ActionKind,
        ac: &mut ActionContext,
        f: impl FnOnce(AsyncActionContext) -> Fut + 'static,
    ) where
        Fut: Future<Output = ()> + 'static,
    {
        let aac_source = AsyncActionContextSource::new();
        let aac = aac_source.context();
        let future = aac_source.call(ac, || f(aac));
        let action = Rc::new(Self {
            kind,
            aac_source,
            data: RefCell::new(None),
        });
        let id = ac.0.rt.async_actions.insert(action.clone());
        *action.data.borrow_mut() = Some(AsyncActionData {
            id,
            waker: WakeReaction::AsyncAction(action.clone()).into_waker(),
            future: Box::pin(future),
        });
        action.next(ac);
    }
    fn call(
        self: &Rc<Self>,
        ac: &mut ActionContext,
        f: impl FnOnce(&mut Option<AsyncActionData>) -> Option<usize>,
    ) {
        let id_remove = self.aac_source.call(ac, || f(&mut self.data.borrow_mut()));
        if let Some(id_remove) = id_remove {
            ac.0.rt.async_actions.remove(id_remove);
        }
    }

    fn cancel(self: &Rc<Self>, ac: &mut ActionContext) {
        self.call(ac, |data| Some(data.take()?.id))
    }
    fn next(self: Rc<Self>, ac: &mut ActionContext) {
        self.call(ac, |data| {
            let d = data.as_mut()?;
            let mut cx = Context::from_waker(&d.waker);
            if d.future.as_mut().poll(&mut cx).is_ready() {
                Some(data.take()?.id)
            } else {
                None
            }
        });
    }
    fn to_action(self: &Rc<Self>) -> Action {
        Action::from_rc_fn(self.clone(), Self::next)
    }
}

struct AsyncActionData {
    future: Pin<Box<dyn Future<Output = ()>>>,
    waker: Waker,
    id: usize,
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
    AsyncAction(Rc<AsyncAction>),
}
impl WakeReaction {
    fn into_waker(self) -> Waker {
        Globals::with(|sc| sc.wakes.insert(self)).into()
    }
}
pub fn waker_from_sink(sink: Weak<impl BindSink>, slot: Slot) -> Waker {
    WakeReaction::Notify(NotifyReaction { sink, slot }).into_waker()
}

#[derive(Clone, Default)]
struct WakeRequests(Arc<Mutex<RawWakeRequests>>);

#[derive(Default)]
struct RawWakeRequests {
    wakes: Vec<usize>,
    drops: Vec<usize>,
    waker: Option<Waker>,
}
impl RawWakeRequests {
    fn wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

struct RawWake {
    requests: WakeRequests,
    key: usize,
}
impl RawWake {
    fn new(requests: &WakeRequests, key: usize) -> Arc<Self> {
        Arc::new(RawWake {
            requests: requests.clone(),
            key,
        })
    }
    fn requests(&self) -> MutexGuard<'_, RawWakeRequests> {
        self.requests.0.lock().unwrap()
    }
}

impl Wake for RawWake {
    fn wake(self: Arc<Self>) {
        let mut requests = self.requests();
        requests.wakes.push(self.key);
        requests.wake();
    }
}
impl Drop for RawWake {
    fn drop(&mut self) {
        self.requests().drops.push(self.key);
    }
}

struct AsyncActionContextSource(Rc<RefCell<*mut RawRuntime>>);

impl AsyncActionContextSource {
    fn new() -> Self {
        Self(Rc::new(RefCell::new(null_mut())))
    }
    fn call<T>(&self, ac: &mut ActionContext, f: impl FnOnce() -> T) -> T {
        let p: *mut RawRuntime = &mut ac.0;
        assert!(self.0.borrow().is_null());
        *self.0.borrow_mut() = p;
        let ret = f();
        assert!(*self.0.borrow() == p);
        *self.0.borrow_mut() = null_mut();
        ret
    }
    fn context(&self) -> AsyncActionContext {
        AsyncActionContext(self.0.clone())
    }
}

/// Context for asynchronous state change.
pub struct AsyncActionContext(Rc<RefCell<*mut RawRuntime>>);

impl AsyncActionContext {
    pub fn call<T>(&self, f: impl FnOnce(&mut ActionContext) -> T) -> T {
        let mut b = self.0.borrow_mut();
        assert!(
            !b.is_null(),
            "`AsyncActionContext` cannot be used after being moved."
        );
        unsafe { f((**b).ac()) }
    }
}

/// Operations that do not modify state.
pub struct Reaction(RawReaction);

impl Reaction {
    pub fn new(f: impl FnOnce(&mut ReactionContext<'_, '_>) + 'static) -> Self {
        Reaction(RawReaction::Box(Box::new(f)))
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

    pub fn schedule_with(self, kind: ReactionKind) {
        Globals::schedule_reaction(kind, self)
    }
    pub fn schedule(self) {
        self.schedule_with(ReactionKind::default());
    }
    fn run(self, rc: &mut ReactionContext<'_, '_>) {
        match self.0 {
            RawReaction::Box(f) => f(rc),
            RawReaction::Rc { this, f } => f(this, rc),
            RawReaction::Weak { this, f } => f(this, rc),
        }
    }
}

enum RawReaction {
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

/// kind of reactions performed by the reactive runtime.
#[derive(Clone, Copy, Display, Debug, Ex)]
#[derive_ex(PartialEq, Eq, Hash, Default)]
#[display("{id}: {name}")]
#[default(Self::new(0, "<default>"))]
pub struct ReactionKind {
    id: i8,
    #[eq(ignore)]
    name: &'static str,
}
impl ReactionKind {
    pub const fn new(id: i8, name: &'static str) -> Self {
        Self { id, name }
    }
    pub fn is_registered(&self) -> bool {
        Globals::with(|g| g.is_reaction_kind_registered(*self))
    }
}

/// Kind of actions performed by the reactive runtime.
#[derive(Clone, Copy, Display, Debug, Ex)]
#[derive_ex(PartialEq, Eq, Hash, Default)]
#[display("{id}: {name}")]
#[default(Self::new(0, "<default>"))]
pub struct ActionKind {
    id: i8,
    #[eq(ignore)]
    name: &'static str,
}
impl ActionKind {
    pub const fn new(id: i8, name: &'static str) -> Self {
        Self { id, name }
    }
    pub fn is_registered(&self) -> bool {
        Globals::with(|g| g.is_action_kind_registered(*self))
    }
}

#[cfg(test)]
mod tests;
