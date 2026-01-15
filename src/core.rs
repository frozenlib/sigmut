use core::panic;
use std::{
    any::Any,
    cell::{Ref, RefCell},
    cmp::{max, min},
    collections::HashSet,
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

use crate::utils::isize_map::ISizeMap;

thread_local! {
    static GLOBALS: RefCell<Globals> = RefCell::new(Globals::new());
}

struct Globals {
    is_runtime_exists: bool,
    runtime: Option<Box<RawRuntime>>,
    unbinds: Vec<SourceBindingsData>,
    actions: Buckets<Action>,
    notifys: Vec<NotifyTask>,
    need_wake: bool,
    wakes: WakeTable,
    tasks: Buckets<Task>,
    registered_task_kinds: HashSet<i8>,
    registered_action_kinds: HashSet<i8>,
}
impl Globals {
    fn new() -> Self {
        Self {
            is_runtime_exists: false,
            runtime: None,
            unbinds: Vec::new(),
            actions: Buckets::new(),
            notifys: Vec::new(),
            need_wake: false,
            wakes: WakeTable::default(),
            tasks: Buckets::new(),
            registered_task_kinds: HashSet::new(),
            registered_action_kinds: HashSet::new(),
        }
    }
    fn with<T>(f: impl FnOnce(&mut Self) -> T) -> T {
        GLOBALS.with(|g| f(&mut g.borrow_mut()))
    }
    fn try_with<T>(f: impl FnOnce(&mut Self) -> T) -> Result<T, AccessError> {
        GLOBALS.try_with(|g| f(&mut g.borrow_mut()))
    }
    fn schedule_task(kind: TaskKind, task: Task) {
        Self::with(|g| {
            g.assert_exists();
            if !g.is_task_kind_registered(kind) {
                panic!("`TaskKind` {} is not registered.", kind);
            }
            g.tasks.push(kind.id, task);
            g.wake();
        })
    }

    fn schedule_action(kind: ActionKind, action: Action) {
        Self::with(|g| {
            g.assert_exists();
            if !g.is_action_kind_registered(kind) {
                panic!("`ActionKind` {} is not registered.", kind);
            }
            g.actions.push(kind.id, action);
            g.wake();
        })
    }
    fn get_notifys(notifys: &mut Vec<NotifyTask>) -> bool {
        Self::with(|g| {
            g.apply_wake();
            swap(notifys, &mut g.notifys);
        });
        !notifys.is_empty()
    }

    fn get_tasks(kind: Option<TaskKind>, tasks: &mut Vec<Task>) {
        Self::with(|g| {
            g.tasks.drain(kind.map(|k| k.id), tasks);
        })
    }
    fn get_actions(kind: Option<ActionKind>, actions: &mut Vec<Action>) -> bool {
        Self::with(|g| {
            g.apply_wake();
            let was_empty = g.actions.is_empty();
            g.actions.drain(kind.map(|k| k.id), actions);
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
        self.notifys.push(NotifyTask { sink, slot });
        self.wake();
    }
    fn apply_wake(&mut self) {
        let mut requests = self.wakes.requests.0.lock().unwrap();
        for key in requests.drops.drain(..) {
            self.wakes.tasks.remove(key);
        }
        for key in requests.wakes.drain(..) {
            if let Some(task) = self.wakes.tasks.get(key) {
                match task {
                    WakeTask::Notify(task) => {
                        self.notifys.push(task.clone());
                    }
                    WakeTask::AsyncAction(action) => self
                        .actions
                        .push(ActionKind::default().id, action.to_action()),
                }
            }
        }
    }
    fn wait_for_ready(&mut self, cx: &Context) -> Poll<()> {
        self.need_wake = false;
        if !self.notifys.is_empty()
            || !self.actions.is_empty()
            || !self.tasks.is_empty()
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
        self.registered_task_kinds.clear();
        self.registered_action_kinds.clear();
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

    fn register_task_kind(&mut self, kind: TaskKind) {
        self.assert_exists();
        self.registered_task_kinds.insert(kind.id);
    }
    fn register_action_kind(&mut self, kind: ActionKind) {
        self.assert_exists();
        self.registered_action_kinds.insert(kind.id);
    }
    fn is_task_kind_registered(&self, kind: TaskKind) -> bool {
        kind.id == 0 || self.registered_task_kinds.contains(&kind.id)
    }
    fn is_action_kind_registered(&self, kind: ActionKind) -> bool {
        kind.id == 0 || self.registered_action_kinds.contains(&kind.id)
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
                tasks_buffer: Vec::new(),
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
    pub fn register_task_kind(kind: TaskKind) {
        Globals::with(|g| g.register_task_kind(kind))
    }

    pub fn ac(&mut self) -> &mut ActionContext {
        self.as_raw().ac()
    }
    pub fn uc(&mut self) -> UpdateContext<'_> {
        self.as_raw().uc()
    }
    pub fn sc(&mut self) -> SignalContext<'_> {
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

    /// Dispatch scheduled tasks for the specified kind.
    ///
    /// Returns `true` if any task was dispatched.
    pub fn dispatch_tasks(&mut self, kind: TaskKind) -> bool {
        self.as_raw().dispatch_tasks_with(Some(kind))
    }

    /// Dispatch scheduled tasks for all kinds.
    ///
    /// Returns `true` if any task was dispatched.
    pub fn dispatch_all_tasks(&mut self) -> bool {
        self.as_raw().dispatch_tasks_with(None)
    }

    /// Dispatch scheduled discards.
    ///
    /// Returns `true` if any discard was dispatched.
    pub fn dispatch_discards(&mut self) -> bool {
        self.as_raw().dispatch_discards()
    }

    /// Flush all pending operations.
    ///
    /// Repeats [`dispatch_all_actions`](Self::dispatch_all_actions), [`dispatch_all_tasks`](Self::dispatch_all_tasks),
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
    notifys_buffer: Vec<NotifyTask>,
    actions_buffer: Vec<Action>,
    tasks_buffer: Vec<Task>,
    unbinds_buffer: Vec<SourceBindingsData>,
}
impl RawRuntime {
    pub fn ac(&mut self) -> &mut ActionContext {
        ActionContext::new(self)
    }
    fn nc(&mut self) -> &mut NotifyContext {
        self.ac().nc()
    }
    fn uc(&mut self) -> UpdateContext<'_> {
        self.apply_notify();
        self.uc_raw()
    }
    fn uc_raw(&mut self) -> UpdateContext<'_> {
        UpdateContext(self.sc_raw())
    }
    fn sc(&mut self) -> SignalContext<'_> {
        self.apply_notify();
        self.sc_raw()
    }
    fn sc_raw(&mut self) -> SignalContext<'_> {
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

    fn dispatch_tasks_with(&mut self, kind: Option<TaskKind>) -> bool {
        self.apply_notify();
        let mut tasks = take(&mut self.tasks_buffer);
        Globals::get_tasks(kind, &mut tasks);
        let handled = !tasks.is_empty();
        for task in tasks.drain(..) {
            task.run(&mut self.uc_raw());
        }
        self.tasks_buffer = tasks;
        handled
    }
    fn apply_unbind(&mut self) -> bool {
        let mut handled = false;
        let mut unbinds = take(&mut self.unbinds_buffer);
        while Globals::swap_source_bindings(|g| &mut g.unbinds, &mut unbinds) {
            for unbind in unbinds.drain(..) {
                for sb in unbind {
                    sb.unbind(&mut self.uc_raw());
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
            if let Some(task) = self.rt.discards.pop() {
                task.run(&mut self.uc_raw());
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
            if self.dispatch_tasks_with(None) {
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
    discards: Vec<Task>,
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

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum NotifyLevel {
    Dirty,
    MaybeDirty,
}
impl NotifyLevel {
    pub fn with_filter(self, filter: bool) -> Self {
        if filter {
            NotifyLevel::MaybeDirty
        } else {
            self
        }
    }
    pub fn is_dirty(self) -> bool {
        self == NotifyLevel::Dirty
    }
    pub fn is_maybe_dirty(self) -> bool {
        self == NotifyLevel::MaybeDirty
    }
}

impl From<NotifyLevel> for Dirty {
    fn from(value: NotifyLevel) -> Self {
        match value {
            NotifyLevel::Dirty => Dirty::Dirty,
            NotifyLevel::MaybeDirty => Dirty::MaybeDirty,
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
    fn check(&self, uc: &mut UpdateContext) -> bool {
        self.source.clone().check(self.slot, self.key, uc)
    }
    fn unbind(self, uc: &mut UpdateContext) {
        self.source.unbind(self.slot, self.key, uc);
    }
    fn rebind(self, sc: &mut SignalContext) {
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
    pub fn check(&self, uc: &mut UpdateContext) -> bool {
        for source in &self.0 {
            if source.check(uc) {
                return true;
            }
        }
        false
    }
    fn check_with(&mut self, dirty: &mut Dirty, uc: &mut UpdateContext) -> bool {
        if *dirty == Dirty::MaybeDirty {
            *dirty = Dirty::from_is_dirty(self.check(uc));
        }
        *dirty == Dirty::Dirty
    }

    pub fn update<T>(
        &mut self,
        sink: Weak<dyn BindSink>,
        slot: Slot,
        reset: bool,
        f: impl FnOnce(&mut SignalContext) -> T,
        uc: &mut UpdateContext,
    ) -> T {
        let sources_len = if reset { 0 } else { self.0.len() };
        let mut sink = Sink {
            sink,
            slot,
            sources: take(self),
            sources_len,
        };
        let mut sc = SignalContext {
            rt: uc.0.rt,
            bump: uc.0.bump,
            sink: Some(&mut sink),
        };

        let ret = f(&mut sc);
        *self = sink.sources;
        for b in self.0.drain(sink.sources_len..) {
            b.unbind(uc);
        }
        ret
    }
    pub fn clear(&mut self, uc: &mut UpdateContext) {
        for b in self.0.drain(..) {
            b.unbind(uc)
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
    fn notify(&self, level: NotifyLevel, nc: &mut NotifyContext) {
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
    pub fn bind(&mut self, this: Rc<dyn BindSource>, this_slot: Slot, sc: &mut SignalContext) {
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
            old.unbind(sc.uc());
        }
    }
    pub fn rebind(
        &mut self,
        this: Rc<dyn BindSource>,
        this_slot: Slot,
        key: BindKey,
        sc: &mut SignalContext,
    ) {
        if let Some(sink) = &mut sc.sink {
            self.0[key.0].slot = sink.slot;
            if let Some(old) = sink.push(SourceBinding {
                source: this,
                slot: this_slot,
                key,
            }) {
                old.unbind(sc.uc());
            }
        } else {
            self.unbind(key, sc.uc());
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn is_dirty(&self, key: BindKey, _uc: &mut UpdateContext) -> bool {
        match self.0[key.0].dirty {
            Dirty::Clean => false,
            Dirty::MaybeDirty => panic!("`is_dirty` called before `update()`"),
            Dirty::Dirty => true,
        }
    }
    /// Unbinds the dependency identified by the given `key`.
    pub fn unbind(&mut self, key: BindKey, _uc: &mut UpdateContext) {
        self.0.remove(key.0);
    }

    pub fn notify(&mut self, level: NotifyLevel, nc: &mut NotifyContext) {
        self.0.optimize();
        for binding in self.0.values_mut() {
            if binding.dirty.needs_notify() {
                binding.notify(level, nc);
            }
            binding.dirty.apply_notify(level);
        }
    }
    pub fn update(&mut self, is_dirty: bool, _uc: &mut UpdateContext) {
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

#[repr(transparent)]
pub struct UpdateContext<'s>(SignalContext<'s>);

impl<'s> UpdateContext<'s> {
    fn new<'a>(sc: &'a mut SignalContext<'s>) -> &'a mut Self {
        unsafe { transmute(sc) }
    }

    /// Register a task to discard the cache.
    ///
    /// Registered tasks are called when [`Runtime::dispatch_discards`] is called.
    pub fn schedule_discard(&mut self, discard: Task) {
        self.0.rt.discards.push(discard)
    }

    /// Call a function with a [`SignalContext`] that does not track dependencies.
    pub fn sc_with<T>(&mut self, f: impl FnOnce(&mut SignalContext<'s>) -> T) -> T {
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
pub struct SignalContext<'s> {
    rt: &'s mut RuntimeData,
    bump: &'s Bump,
    sink: Option<&'s mut Sink>,
}

impl<'s> SignalContext<'s> {
    pub fn uc(&mut self) -> &mut UpdateContext<'s> {
        UpdateContext::new(self)
    }

    /// Call a function with a [`SignalContext`] that does not track dependencies.
    pub fn untrack<T>(&mut self, f: impl FnOnce(&mut SignalContext<'s>) -> T) -> T {
        struct UntrackGuard<'s, 'a> {
            sc: &'a mut SignalContext<'s>,
            sink: Option<&'s mut Sink>,
        }
        impl Drop for UntrackGuard<'_, '_> {
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
    fn notify(self: Rc<Self>, slot: Slot, level: NotifyLevel, nc: &mut NotifyContext);
}

/// A trait for types that can hold a state and be monitored for changes.
pub trait BindSource: 'static {
    /// Checks if this source has been modified since the last check.
    ///
    /// Returns `true` if the source is dirty (has changes), `false` if clean (no changes).
    fn check(self: Rc<Self>, slot: Slot, key: BindKey, uc: &mut UpdateContext) -> bool;
    fn unbind(self: Rc<Self>, slot: Slot, key: BindKey, uc: &mut UpdateContext);
    fn rebind(self: Rc<Self>, slot: Slot, key: BindKey, sc: &mut SignalContext);
}

#[derive(Clone)]
struct NotifyTask {
    sink: Weak<dyn BindSink>,
    slot: Slot,
}
impl NotifyTask {
    fn call_notify(&self, nc: &mut NotifyContext) {
        if let Some(sink) = self.sink.upgrade() {
            sink.notify(self.slot, NotifyLevel::Dirty, nc)
        }
    }
}

/// Context for changing state.
#[repr(transparent)]
pub struct ActionContext(RawRuntime);

impl ActionContext {
    fn new(rt: &mut RawRuntime) -> &mut Self {
        unsafe { transmute(rt) }
    }
    pub fn nc(&mut self) -> &mut NotifyContext {
        NotifyContext::new(self)
    }
    pub fn sc(&mut self) -> SignalContext<'_> {
        self.0.sc()
    }
}

/// Spawns a new action.
pub fn spawn_action(f: impl FnOnce(&mut ActionContext) + 'static) {
    Action::new(f).schedule()
}

/// Spawns a new asynchronous action.
pub fn spawn_action_async(f: impl AsyncFnOnce(&mut AsyncActionContext) + 'static) {
    spawn_action(|ac| {
        AsyncAction::start(ac, |mut ac| async move {
            f(&mut ac).await;
        })
    })
}

/// Spawns a new action without heap allocation.
///
/// `f` should be of a zero-sized type.
/// If `f` is not a zero-sized type, heap allocation will occur.
pub fn spawn_action_rc<T: Any>(
    this: Rc<T>,
    f: impl Fn(Rc<T>, &mut ActionContext) + Copy + 'static,
) {
    Action::from_rc_fn(this, f).schedule()
}

/// Represents an action to be executed by the runtime.
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
    aac_source: AsyncActionContextSource,
    data: RefCell<Option<AsyncActionData>>,
}
impl AsyncAction {
    fn start<Fut>(ac: &mut ActionContext, f: impl FnOnce(AsyncActionContext) -> Fut + 'static)
    where
        Fut: Future<Output = ()> + 'static,
    {
        let aac_source = AsyncActionContextSource::new();
        let aac = aac_source.context();
        let future = aac_source.call(ac, || f(aac));
        let action = Rc::new(Self {
            aac_source,
            data: RefCell::new(None),
        });
        let id = ac.0.rt.async_actions.insert(action.clone());
        *action.data.borrow_mut() = Some(AsyncActionData {
            id,
            waker: WakeTask::AsyncAction(action.clone()).into_waker(),
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
    tasks: SlabMap<WakeTask>,
    requests: WakeRequests,
}

impl WakeTable {
    fn insert(&mut self, task: WakeTask) -> Arc<RawWake> {
        RawWake::new(&self.requests, self.tasks.insert(task))
    }
}
enum WakeTask {
    Notify(NotifyTask),
    AsyncAction(Rc<AsyncAction>),
}
impl WakeTask {
    fn into_waker(self) -> Waker {
        Globals::with(|sc| sc.wakes.insert(self)).into()
    }
}
pub fn waker_from_sink(sink: Weak<impl BindSink>, slot: Slot) -> Waker {
    WakeTask::Notify(NotifyTask { sink, slot }).into_waker()
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

pub struct Task(RawTask);

impl Task {
    pub fn new(f: impl FnOnce(&mut UpdateContext) + 'static) -> Self {
        Task(RawTask::Box(Box::new(f)))
    }
    pub fn from_rc_fn<T: Any>(
        this: Rc<T>,
        f: impl Fn(Rc<T>, &mut UpdateContext) + Copy + 'static,
    ) -> Self {
        Task(RawTask::Rc {
            this,
            f: Box::new(move |this, uc| f(this.downcast().unwrap(), uc)),
        })
    }
    pub fn from_weak_fn<T: Any>(
        this: Weak<T>,
        f: impl Fn(Rc<T>, &mut UpdateContext) + Copy + 'static,
    ) -> Self {
        Task(RawTask::Weak {
            this,
            f: Box::new(move |this, uc| {
                if let Some(this) = this.upgrade() {
                    f(this.downcast().unwrap(), uc)
                }
            }),
        })
    }

    pub fn schedule_with(self, kind: TaskKind) {
        Globals::schedule_task(kind, self)
    }
    pub fn schedule(self) {
        self.schedule_with(TaskKind::default());
    }
    fn run(self, uc: &mut UpdateContext) {
        match self.0 {
            RawTask::Box(f) => f(uc),
            RawTask::Rc { this, f } => f(this, uc),
            RawTask::Weak { this, f } => f(this, uc),
        }
    }
}

enum RawTask {
    Box(Box<dyn FnOnce(&mut UpdateContext)>),
    Rc {
        this: Rc<dyn Any>,
        #[allow(clippy::type_complexity)]
        f: Box<dyn Fn(Rc<dyn Any>, &mut UpdateContext)>,
    },
    Weak {
        this: Weak<dyn Any>,
        #[allow(clippy::type_complexity)]
        f: Box<dyn Fn(Weak<dyn Any>, &mut UpdateContext)>,
    },
}

/// kind of tasks performed by the reactive runtime.
#[derive(Clone, Copy, Display, Debug, Ex)]
#[derive_ex(PartialEq, Eq, Hash, Default)]
#[display("{id}: {name}")]
#[default(Self::new(0, "<default>"))]
pub struct TaskKind {
    id: i8,
    #[eq(ignore)]
    name: &'static str,
}
impl TaskKind {
    pub const fn new(id: i8, name: &'static str) -> Self {
        Self { id, name }
    }
    pub fn is_registered(&self) -> bool {
        Globals::with(|g| g.is_task_kind_registered(*self))
    }
    pub fn assert_registered(&self) {
        if !self.is_registered() {
            panic!("`TaskKind` {} is not registered.", self);
        }
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
    pub fn assert_registered(&self) {
        if !self.is_registered() {
            panic!("`ActionKind` {} is not registered.", self);
        }
    }
}

#[derive(Ex)]
#[derive_ex(Default)]
#[default(Self::new())]
struct Buckets<T> {
    buckets: ISizeMap<Vec<T>>,
    start: isize,
    last: isize,
}
impl<T> Buckets<T> {
    fn new() -> Self {
        Self {
            buckets: ISizeMap::new(),
            start: isize::MAX,
            last: isize::MIN,
        }
    }

    fn is_empty(&self) -> bool {
        self.start == isize::MAX
    }
    fn set_empty(&mut self) {
        self.start = isize::MAX;
        self.last = isize::MIN;
    }
    fn push(&mut self, id: i8, item: T) {
        let index = id as isize;
        self.buckets[index].push(item);
        self.start = min(self.start, index);
        self.last = max(self.last, index);
    }
    fn drain(&mut self, id: Option<i8>, to: &mut Vec<T>) {
        if let Some(id) = id {
            let index = id as isize;
            if let Some(bucket) = self.buckets.get_mut(index) {
                to.append(bucket)
            }
            if self.start == index {
                self.start += 1;
            }
            if self.start > self.last {
                self.set_empty();
            }
        } else {
            for index in self.start..=self.last {
                to.append(&mut self.buckets[index])
            }
            self.set_empty();
        }
    }
}

#[non_exhaustive]
#[derive(Display, Debug)]
#[display("detect cyclic dependency")]
pub struct CyclicError {}

impl std::error::Error for CyclicError {}

#[cfg(test)]
mod tests;
