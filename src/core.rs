use core::panic;
use std::{
    any::Any,
    cell::{Ref, RefCell},
    cmp::{max, min},
    future::{poll_fn, Future},
    mem::{replace, swap, take, transmute},
    ops::{BitOr, BitOrAssign},
    pin::Pin,
    ptr::null_mut,
    rc::{Rc, Weak},
    result::Result,
    sync::{Arc, Mutex, MutexGuard},
    task::{Context, Poll, Wake, Waker},
    thread::AccessError,
};

use bumpalo::Bump;
use derive_ex::{derive_ex, Ex};
use parse_display::Display;
use slabmap::SlabMap;

mod async_signal_context;
mod source_binder;
mod state_ref;
mod state_ref_builder;

pub use async_signal_context::*;
pub use source_binder::SourceBinder;
pub use state_ref::StateRef;
pub use state_ref_builder::StateRefBuilder;

use crate::utils::isize_map::ISizeMap;

thread_local! {
    static GLOBALS: RefCell<Globals> = RefCell::new(Globals::new());
}

struct Globals {
    is_runtime_exists: bool,
    unbinds: Vec<Vec<SourceBinding>>,
    actions: Vec<Action>,
    notifys: Vec<NotifyTask>,
    need_wake: bool,
    wakes: WakeTable,
    tasks: Tasks,
}
impl Globals {
    fn new() -> Self {
        Self {
            is_runtime_exists: false,
            unbinds: Vec::new(),
            actions: Vec::new(),
            notifys: Vec::new(),
            need_wake: false,
            wakes: WakeTable::default(),
            tasks: Tasks::new(),
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
            g.tasks.push(kind, task);
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
            g.tasks.drain(kind, tasks);
        })
    }
    fn get_actions(actions: &mut Vec<Action>) -> bool {
        Self::with(|g| {
            g.apply_wake();
            swap(actions, &mut g.actions);
        });
        !actions.is_empty()
    }

    fn swap_vec<T>(f: impl FnOnce(&mut Self) -> &mut Vec<T>, values: &mut Vec<T>) -> bool {
        Self::with(|g| swap(f(g), values));
        !values.is_empty()
    }
    fn assert_exists(&self) {
        if !self.is_runtime_exists {
            panic!("`Runtime` is not created.");
        }
    }

    fn push_action(&mut self, action: Action) {
        self.assert_exists();
        self.actions.push(action);
        self.wake();
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
                    WakeTask::AsyncAction(action) => self.actions.push(action.to_action()),
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
    }

    fn wake(&mut self) {
        if !self.need_wake {
            return;
        }
        self.need_wake = false;
        self.wakes.requests.0.lock().unwrap().wake();
    }
}

/// Reactive runtime.
#[derive_ex(Default)]
#[default(Self::new())]
pub struct Runtime {
    rt: RawRuntime,
    bump: Bump,
    notifys_buffer: Vec<NotifyTask>,
    actions_buffer: Vec<Action>,
    tasks_buffer: Vec<Task>,
    unbinds_buffer: Vec<Vec<SourceBinding>>,
}
impl Runtime {
    pub fn new() -> Self {
        if Globals::with(|g| replace(&mut g.is_runtime_exists, true)) {
            panic!("Only one `Runtime` can exist in the same thread at the same time.");
        };
        Self {
            rt: RawRuntime::new(),
            bump: Bump::new(),
            notifys_buffer: Vec::new(),
            actions_buffer: Vec::new(),
            tasks_buffer: Vec::new(),
            unbinds_buffer: Vec::new(),
        }
    }

    pub fn ac(&mut self) -> &mut ActionContext {
        ActionContext::new(self)
    }
    fn nc(&mut self) -> &mut NotifyContext {
        self.ac().nc()
    }
    fn uc(&mut self) -> UpdateContext {
        UpdateContext(self.sc_raw())
    }
    pub fn sc(&mut self) -> SignalContext {
        self.apply_notify();
        let sc = self.sc_raw();
        sc
    }
    fn sc_raw(&mut self) -> SignalContext {
        SignalContext {
            rt: &mut self.rt,
            bump: &self.bump,
            sink: None,
        }
    }

    /// Perform scheduled actions.
    ///
    /// Returns `true` if any action was performed.
    pub fn run_actions(&mut self) -> bool {
        let mut handled = false;
        let mut actions = take(&mut self.actions_buffer);
        while Globals::get_actions(&mut actions) {
            for action in actions.drain(..) {
                action.call(self.ac());
                handled = true;
            }
        }
        self.actions_buffer = actions;
        handled
    }

    /// Perform scheduled tasks.
    ///
    /// If `kind` is `None`, all tasks are executed.
    ///
    /// Returns `true` if any task was performed.
    pub fn run_tasks(&mut self, kind: Option<TaskKind>) -> bool {
        self.apply_notify();
        let mut tasks = take(&mut self.tasks_buffer);
        Globals::get_tasks(kind, &mut tasks);
        let handled = !tasks.is_empty();
        for task in tasks.drain(..) {
            task.run(&mut self.uc());
        }
        self.tasks_buffer = tasks;
        handled
    }
    fn apply_unbind(&mut self) -> bool {
        let mut handled = false;
        let mut unbinds = take(&mut self.unbinds_buffer);
        while Globals::swap_vec(|g| &mut g.unbinds, &mut unbinds) {
            for unbind in unbinds.drain(..) {
                for sb in unbind {
                    sb.unbind(&mut self.uc());
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

    /// Perform scheduled discards.
    ///
    /// Returns `true` if any discard was performed.
    pub fn run_discards(&mut self) -> bool {
        let mut handled = false;
        loop {
            if let Some(task) = self.rt.discards.pop() {
                task.call_discard(&mut self.uc());
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

    /// Repeat until there are no more processes to do
    /// [`run_actions`](Self::run_actions), [`run_tasks`](Self::run_tasks), or [`run_discards`](Self::run_discards).
    pub fn update(&mut self) {
        loop {
            if self.run_actions() {
                continue;
            }
            if self.run_tasks(None) {
                continue;
            }
            if self.run_discards() {
                continue;
            }
            break;
        }
    }

    /// Wait while there is no process to be executed by [`update`](Self::update).
    pub async fn wait_for_ready(&mut self) {
        poll_fn(|cx| Globals::with(|g| g.wait_for_ready(cx))).await
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

impl Drop for Runtime {
    fn drop(&mut self) {
        self.cancel_async_actions();
        Globals::with(|g| g.finish_runtime());
    }
}

struct RawRuntime {
    discards: Vec<DiscardTask>,
    async_actions: SlabMap<Rc<AsyncAction>>,
}

impl RawRuntime {
    pub fn new() -> Self {
        Self {
            discards: Vec::new(),
            async_actions: SlabMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub enum Dirty {
    Clean,
    MaybeDirty,
    Dirty,
}
impl Dirty {
    pub fn from_is_dirty(is_dirty: bool) -> Self {
        if is_dirty {
            Dirty::Dirty
        } else {
            Dirty::Clean
        }
    }
    pub fn is_clean(self) -> bool {
        self == Dirty::Clean
    }
}

impl BitOr for Dirty {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        max(self, rhs)
    }
}
impl BitOrAssign for Dirty {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs;
    }
}

impl BitOr<DirtyOrMaybeDirty> for Dirty {
    type Output = Self;
    fn bitor(self, rhs: DirtyOrMaybeDirty) -> Self {
        max(self, rhs.into())
    }
}
impl BitOrAssign<DirtyOrMaybeDirty> for Dirty {
    fn bitor_assign(&mut self, rhs: DirtyOrMaybeDirty) {
        *self = *self | rhs;
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum DirtyOrMaybeDirty {
    Dirty,
    MaybeDirty,
}
impl DirtyOrMaybeDirty {
    pub fn with_filter(self, filter: bool) -> Self {
        if filter {
            DirtyOrMaybeDirty::MaybeDirty
        } else {
            self
        }
    }
}

impl From<DirtyOrMaybeDirty> for Dirty {
    fn from(value: DirtyOrMaybeDirty) -> Self {
        match value {
            DirtyOrMaybeDirty::Dirty => Dirty::Dirty,
            DirtyOrMaybeDirty::MaybeDirty => Dirty::MaybeDirty,
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

#[derive(Default)]
pub struct SourceBindings(Vec<SourceBinding>);

impl SourceBindings {
    pub fn new() -> Self {
        Self::default()
    }
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
    fn notify(&self, dirty: DirtyOrMaybeDirty, nc: &mut NotifyContext) {
        if let Some(node) = self.sink.upgrade() {
            node.notify(self.slot, dirty, nc)
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
        if let Some(source_old) = sink.sources.0.get(sources_index) {
            if source_old.is_same(&this, this_slot) {
                sink.sources_len += 1;
                self.0[source_old.key.0].dirty = Dirty::Clean;
                return;
            }
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

    pub fn notify(&mut self, dirty: DirtyOrMaybeDirty, nc: &mut NotifyContext) {
        self.0.optimize();
        for binding in self.0.values_mut() {
            if binding.dirty.is_clean() {
                binding.notify(dirty, nc);
            }
            binding.dirty |= dirty;
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
    /// Registered tasks are called when [`Runtime::run_discards`] is called.
    pub fn schedule_discard(&mut self, discard: Rc<dyn Discard>, slot: Slot) {
        self.0.rt.discards.push(DiscardTask {
            node: discard,
            slot,
        })
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
    rt: &'s mut RawRuntime,
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

pub trait BindSink: 'static {
    fn notify(self: Rc<Self>, slot: Slot, dirty: DirtyOrMaybeDirty, nc: &mut NotifyContext);
}

pub trait BindSource: 'static {
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
            sink.notify(self.slot, DirtyOrMaybeDirty::Dirty, nc)
        }
    }
}

pub trait Discard {
    fn discard(self: Rc<Self>, slot: Slot, uc: &mut UpdateContext);
}
struct DiscardTask {
    node: Rc<dyn Discard>,
    slot: Slot,
}
impl DiscardTask {
    fn call_discard(self, uc: &mut UpdateContext) {
        self.node.discard(self.slot, uc)
    }
}

/// Context for changing state.
#[repr(transparent)]
pub struct ActionContext(Runtime);

impl ActionContext {
    fn new(rt: &mut Runtime) -> &mut Self {
        unsafe { transmute(rt) }
    }
    pub fn nc(&mut self) -> &mut NotifyContext {
        NotifyContext::new(self)
    }
    pub fn sc(&mut self) -> SignalContext {
        self.0.sc()
    }
}

/// Spawns a new action.
pub fn spawn_action(f: impl FnOnce(&mut ActionContext) + 'static) {
    Action::Box(Box::new(f)).schedule()
}

/// Spawns a new asynchronous action.
pub fn spawn_action_async<Fut>(f: impl FnOnce(AsyncActionContext) -> Fut + 'static)
where
    Fut: Future<Output = ()> + 'static,
{
    spawn_action(|ac| AsyncAction::start(ac, f))
}

/// Spawns a new action without heap allocation.
///
/// `f` should be of a zero-sized type.
/// If `f` is not a zero-sized type, heap allocation will occur.
pub fn spawn_action_rc<T: Any>(
    this: Rc<T>,
    f: impl Fn(Rc<T>, &mut ActionContext) + Copy + 'static,
) {
    Action::from_rc(this, f).schedule()
}

#[allow(clippy::type_complexity)]
enum Action {
    Box(Box<dyn FnOnce(&mut ActionContext)>),
    Rc {
        this: Rc<dyn Any>,
        f: Box<dyn Fn(Rc<dyn Any>, &mut ActionContext)>,
    },
}

impl Action {
    fn from_rc<T: Any>(this: Rc<T>, f: impl Fn(Rc<T>, &mut ActionContext) + 'static) -> Self {
        Action::Rc {
            this,
            f: Box::new(move |this, ac| f(this.downcast().unwrap(), ac)),
        }
    }
    fn call(self, ac: &mut ActionContext) {
        match self {
            Action::Box(f) => f(ac),
            Action::Rc { this, f } => f(this, ac),
        }
    }
    fn schedule(self) {
        let _ = Globals::try_with(|g| g.push_action(self));
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
        Action::from_rc(self.clone(), Self::next)
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
    fn requests(&self) -> MutexGuard<RawWakeRequests> {
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

struct AsyncActionContextSource(Rc<RefCell<*mut Runtime>>);

impl AsyncActionContextSource {
    fn new() -> Self {
        Self(Rc::new(RefCell::new(null_mut())))
    }
    fn call<T>(&self, ac: &mut ActionContext, f: impl FnOnce() -> T) -> T {
        let p: *mut Runtime = &mut ac.0;
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
pub struct AsyncActionContext(Rc<RefCell<*mut Runtime>>);

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
}

#[derive(Ex)]
#[derive_ex(Default)]
#[default(Self::new())]
struct Tasks {
    tasks: ISizeMap<Vec<Task>>,
    start: isize,
    last: isize,
}
impl Tasks {
    fn new() -> Self {
        Self {
            tasks: ISizeMap::new(),
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
    fn push(&mut self, kind: TaskKind, task: Task) {
        let index = kind.id as isize;
        self.tasks[index].push(task);
        self.start = min(self.start, index);
        self.last = max(self.last, index);
    }
    fn drain(&mut self, kind: Option<TaskKind>, to: &mut Vec<Task>) {
        if let Some(kind) = kind {
            let index = kind.id as isize;
            if let Some(tasks) = self.tasks.get_mut(index) {
                to.append(tasks)
            }
            if self.start == index {
                self.start += 1;
            }
            if self.start > self.last {
                self.set_empty();
            }
        } else {
            for index in self.start..=self.last {
                to.append(&mut self.tasks[index])
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
