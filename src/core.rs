use core::panic;
use std::{
    any::Any,
    cell::RefCell,
    cmp::max,
    collections::VecDeque,
    future::Future,
    mem::{replace, swap, take, transmute},
    ops::{BitOr, BitOrAssign},
    pin::{pin, Pin},
    ptr::null_mut,
    rc::{Rc, Weak},
    result::Result,
    sync::{Arc, Mutex, MutexGuard},
    task::{Context, Poll, Wake, Waker},
    thread::AccessError,
};

use bumpalo::Bump;
use derive_ex::derive_ex;
use slabmap::SlabMap;

mod async_source_binder;
mod source_binder;
mod state_ref;
mod state_ref_builder;

pub use async_source_binder::AsyncSourceBinder;
pub use source_binder::SourceBinder;
pub use state_ref::StateRef;
pub use state_ref_builder::StateRefBuilder;

use crate::utils::PhantomNotSend;

thread_local! {
    static GLOBALS: RefCell<Globals> = RefCell::new(Globals::new());
}

struct Globals {
    is_runtime_exists: bool,
    phase: usize,
    unbinds: Vec<Vec<SourceBinding>>,
    actions: Vec<Action>,
    notifys: Vec<NotifyTask>,
    wakes: WakeTable,
    wait_for_update_wakers: Vec<Waker>,
    schedulers: SlabMap<Weak<SchedulerData>>,
}
impl Globals {
    fn new() -> Self {
        Self {
            is_runtime_exists: false,
            phase: 0,
            unbinds: Vec::new(),
            actions: Vec::new(),
            notifys: Vec::new(),
            wakes: WakeTable::default(),
            wait_for_update_wakers: Vec::new(),
            schedulers: SlabMap::new(),
        }
    }
    fn with<T>(f: impl FnOnce(&mut Self) -> T) -> T {
        GLOBALS.with(|g| f(&mut g.borrow_mut()))
    }
    fn try_with<T>(f: impl FnOnce(&mut Self) -> T) -> Result<T, AccessError> {
        GLOBALS.try_with(|g| f(&mut g.borrow_mut()))
    }
    fn swap_vec<T>(f: impl FnOnce(&mut Self) -> &mut Vec<T>, values: &mut Vec<T>) -> bool {
        Self::with(|g| swap(f(g), values));
        !values.is_empty()
    }

    fn push_action(&mut self, action: Action) {
        self.actions.push(action);
        self.wake();
    }
    fn push_notify(&mut self, sink: Weak<dyn BindSink>, slot: Slot) {
        self.notifys.push(NotifyTask { sink, slot });
        self.wake();
    }
    fn apply_wake(&mut self) -> bool {
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
        requests.waker = None;
        take(&mut requests.is_wake_main)
    }
    fn poll_wait_for_update(&mut self, phase: usize, cx: &mut Context) -> Poll<()> {
        if !self.is_runtime_exists || phase != self.phase {
            return Poll::Ready(());
        }
        self.wait_for_update_wakers.push(cx.waker().clone());
        self.wake();
        Poll::Pending
    }
    fn resume_wait_for_update(&mut self) -> bool {
        let mut is_used = false;
        self.phase += 1;
        while let Some(waker) = self.wait_for_update_wakers.pop() {
            is_used = true;
            waker.wake();
        }
        is_used
    }
    fn wake(&mut self) {
        self.wakes.requests.0.lock().unwrap().wake();
    }
}

#[derive_ex(Default)]
#[default(Self::new())]
pub struct Runtime {
    rt: RawRuntime,
    bump: Bump,
}
impl Runtime {
    pub fn new() -> Self {
        if Globals::with(|g| replace(&mut g.is_runtime_exists, true)) {
            panic!("Only one `Runtime` can exist in the same thread at the same time.");
        };
        Self {
            rt: RawRuntime::new(),
            bump: Bump::new(),
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

    pub fn run_actions(&mut self) -> bool {
        let mut handled = false;
        let mut actions = take(&mut self.rt.actions_buffer);
        while Globals::swap_vec(|g| &mut g.actions, &mut actions) {
            for action in actions.drain(..) {
                action.call(self.ac());
                handled = true;
            }
        }
        self.rt.actions_buffer = actions;
        handled
    }
    pub fn run_tasks(&mut self, scheduler: &Scheduler) -> bool {
        self.apply_notify();
        scheduler.run(&mut self.uc())
    }
    fn apply_unbind(&mut self) -> bool {
        let mut handled = false;
        let mut unbinds = take(&mut self.rt.unbinds_buffer);
        while Globals::swap_vec(|g| &mut g.unbinds, &mut unbinds) {
            for unbind in unbinds.drain(..) {
                for sb in unbind {
                    sb.unbind(&mut self.uc());
                }
                handled = true;
            }
        }
        self.rt.unbinds_buffer = unbinds;
        handled
    }
    fn apply_notify(&mut self) -> bool {
        let mut handled = self.apply_unbind();
        let mut notifys = take(&mut self.rt.notifys_buffer);
        while Globals::swap_vec(|g| &mut g.notifys, &mut notifys) {
            for notify in notifys.drain(..) {
                notify.call_notify(self.nc());
                handled = true;
            }
        }
        self.rt.notifys_buffer = notifys;
        handled
    }
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
    pub fn update(&mut self) {
        loop {
            if self.run_actions() {
                continue;
            }
            if self.run_tasks(&Scheduler::default()) {
                continue;
            }
            if self.run_discards() {
                continue;
            }
            break;
        }
        self.resume_wait_for_update();
    }
    pub fn resume_wait_for_update(&mut self) {
        Globals::with(|g| g.resume_wait_for_update());
    }

    pub async fn run<Fut: Future>(&mut self, f: impl FnOnce(RuntimeContext) -> Fut) -> Fut::Output {
        let rts = RuntimeContextSource::new();
        let rt = rts.context();
        let fut = pin!(rts.apply(self, move || f(rt)));
        let wake = Globals::with(|g| RawWake::new(&g.wakes.requests, None));
        wake.requests().is_wake_main = true;
        RuntimeMain {
            rt: self,
            rts,
            fut,
            wake,
        }
        .await
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
    fn cancel_tasks(&mut self) {
        Scheduler::default().0.cancel();
        for s in Globals::with(|g| {
            g.schedulers
                .values()
                .filter_map(|s| s.upgrade())
                .collect::<Vec<_>>()
        }) {
            s.cancel();
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.cancel_async_actions();
        self.cancel_tasks();
        Globals::with(|g| g.is_runtime_exists = false);
    }
}

struct RawRuntime {
    discards: Vec<DiscardTask>,
    async_actions: SlabMap<Rc<AsyncAction>>,
    actions_buffer: Vec<Action>,
    unbinds_buffer: Vec<Vec<SourceBinding>>,
    notifys_buffer: Vec<NotifyTask>,
}

impl RawRuntime {
    pub fn new() -> Self {
        Self {
            discards: Vec::new(),
            async_actions: SlabMap::new(),
            actions_buffer: Vec::new(),
            unbinds_buffer: Vec::new(),
            notifys_buffer: Vec::new(),
        }
    }
}

struct RuntimeMain<'a, Fut> {
    rt: &'a mut Runtime,
    rts: RuntimeContextSource,
    wake: Arc<RawWake>,
    fut: Pin<&'a mut Fut>,
}

impl<'a, Fut: Future> Future for RuntimeMain<'a, Fut> {
    type Output = Fut::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        loop {
            if Globals::with(|g| g.apply_wake()) {
                let waker = Waker::from(this.wake.clone());
                let mut cx = Context::from_waker(&waker);
                let p = this.rts.apply(this.rt, || this.fut.as_mut().poll(&mut cx));
                if p.is_ready() {
                    return p;
                }
                continue;
            }
            if this.rt.run_actions() {
                continue;
            }
            // todo
            //
            // if this.rt.update() {
            //     continue;
            // }
            if Globals::with(|rt| rt.resume_wait_for_update()) {
                continue;
            }
            if !this.wake.requests().try_finish_poll(cx) {
                continue;
            }
            return Poll::Pending;
        }
    }
}

struct RuntimeContextSource(Rc<RefCell<*mut Runtime>>);

impl RuntimeContextSource {
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(null_mut())))
    }
    pub fn context(&self) -> RuntimeContext {
        RuntimeContext(self.0.clone())
    }

    fn apply<T>(&self, rt: &mut Runtime, f: impl FnOnce() -> T) -> T {
        let rt: *mut Runtime = rt;
        assert!(self.0.borrow().is_null());
        *self.0.borrow_mut() = rt;
        let ret = f();
        *self.0.borrow_mut() = null_mut();
        ret
    }
}

#[derive(Clone)]
pub struct RuntimeContext(Rc<RefCell<*mut Runtime>>);

impl RuntimeContext {
    pub fn run_actions(&self) -> bool {
        self.with(|rt| rt.run_actions())
    }
    pub fn run_tasks(&self, scheduler: &Scheduler) -> bool {
        self.with(|rt| rt.run_tasks(scheduler))
    }
    pub fn run_discards(&self) -> bool {
        self.with(|rt| rt.run_discards())
    }
    pub fn update(&self) {
        self.with(|rt| rt.update())
    }
    pub fn resume_wait_for_update(&self) {
        self.with(|rt| rt.resume_wait_for_update())
    }

    fn with<T>(&self, f: impl FnOnce(&mut Runtime) -> T) -> T {
        unsafe {
            let p = self.0.borrow_mut();
            if let Some(rt) = p.as_mut() {
                f(rt)
            } else {
                panic!("`RuntimeContext` cannot be used after being moved.");
            }
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
        sink.sources_len += 1;
        if let Some(source_old) = sink.sources.0.get(sources_index) {
            if source_old.is_same(&this, this_slot) {
                self.0[source_old.key.0].dirty = Dirty::Clean;
                return;
            }
        }
        let sink_binding = SinkBinding {
            sink: sink.sink.clone(),
            slot: sink.slot,
            dirty: Dirty::Clean,
        };
        let key = BindKey(self.0.insert(sink_binding));
        let source_binding = SourceBinding {
            source: this,
            slot: this_slot,
            key,
        };
        if sources_index < sink.sources.0.len() {
            replace(&mut sink.sources.0[sources_index], source_binding)
                .unbind(UpdateContext::new(sc));
        } else {
            sink.sources.0.push(source_binding);
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
    ///
    /// Returns `true` if the dependency is successfully unbind and no more dependencies exist.
    pub fn unbind(&mut self, key: BindKey, _uc: &mut UpdateContext) -> bool {
        self.0.remove(key.0).is_some() && self.0.is_empty()
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

#[repr(transparent)]
pub struct UpdateContext<'s>(SignalContext<'s>);

impl<'s> UpdateContext<'s> {
    fn new<'a>(sc: &'a mut SignalContext<'s>) -> &'a mut Self {
        unsafe { transmute(sc) }
    }

    pub fn schedule_discard(&mut self, discard: Rc<dyn Discard>, slot: Slot) {
        self.0.rt.discards.push(DiscardTask {
            node: discard,
            slot,
        })
    }

    pub fn sc_with<T>(&mut self, f: impl FnOnce(&mut SignalContext) -> T) -> T {
        f(&mut SignalContext {
            rt: self.0.rt,
            bump: self.0.bump,
            sink: None,
        })
    }
}

#[repr(transparent)]
pub struct NotifyContext(ActionContext);

impl NotifyContext {
    fn new(ac: &mut ActionContext) -> &mut Self {
        unsafe { transmute(ac) }
    }
}

pub fn schedule_notify(node: Weak<dyn BindSink>, slot: Slot) {
    let _ = Globals::try_with(|rg| rg.push_notify(node, slot));
}

pub struct SignalContext<'s> {
    rt: &'s mut RawRuntime,
    bump: &'s Bump,
    sink: Option<&'s mut Sink>,
}

impl<'s> SignalContext<'s> {
    pub fn uc(&mut self) -> &mut UpdateContext<'s> {
        UpdateContext::new(self)
    }
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
}

pub trait BindSink: 'static {
    fn notify(self: Rc<Self>, slot: Slot, dirty: DirtyOrMaybeDirty, nc: &mut NotifyContext);
}

pub trait BindSource: 'static {
    fn check(self: Rc<Self>, slot: Slot, key: BindKey, uc: &mut UpdateContext) -> bool;
    fn unbind(self: Rc<Self>, slot: Slot, key: BindKey, uc: &mut UpdateContext);
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
        RawWake::new(&self.requests, Some(self.tasks.insert(task)))
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
    is_wake_main: bool,
    waker: Option<Waker>,
}
impl RawWakeRequests {
    fn try_finish_poll(&mut self, cx: &mut Context) -> bool {
        let is_finish = self.drops.is_empty() && self.wakes.is_empty() && !self.is_wake_main;
        if is_finish {
            self.waker = Some(cx.waker().clone());
        }
        is_finish
    }
    fn wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

struct RawWake {
    requests: WakeRequests,
    key: Option<usize>,
}
impl RawWake {
    fn new(requests: &WakeRequests, key: Option<usize>) -> Arc<Self> {
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
        let mut requests = self.requests.0.lock().unwrap();
        if let Some(key) = self.key {
            requests.wakes.push(key);
        } else {
            requests.is_wake_main = true;
        }
        requests.wake();
    }
}
impl Drop for RawWake {
    fn drop(&mut self) {
        if let Some(key) = self.key {
            self.requests.0.lock().unwrap().drops.push(key);
        }
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
struct SignalContextPtr {
    rt: *mut RawRuntime,
    bump: *const Bump,
    sink: Option<*mut Sink>,
}
impl SignalContextPtr {
    fn new(sc: &mut SignalContext) -> Self {
        Self {
            rt: sc.rt,
            bump: sc.bump,
            sink: sc.sink.as_mut().map(|x| *x as *mut _),
        }
    }
}

#[derive_ex(Default)]
struct AsyncSignalContextSource(Rc<RefCell<Option<SignalContextPtr>>>);

impl AsyncSignalContextSource {
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(None)))
    }
    pub fn sc(&self) -> AsyncSignalContext {
        AsyncSignalContext(self.0.clone())
    }
    pub fn with<T>(&self, sc: &mut SignalContext, f: impl FnOnce() -> T) -> T {
        let data = SignalContextPtr::new(sc);
        assert!(self.0.borrow().is_none());
        *self.0.borrow_mut() = Some(data);
        let ret = f();
        assert!(*self.0.borrow() == Some(data));
        *self.0.borrow_mut() = None;
        ret
    }
}

pub struct AsyncSignalContext(Rc<RefCell<Option<SignalContextPtr>>>);

impl AsyncSignalContext {
    pub fn with<T>(&mut self, f: impl FnOnce(&mut SignalContext) -> T) -> T {
        let mut data = self.0.borrow_mut();
        let Some(data) = data.as_mut() else {
            panic!("`AsyncSignalContext` cannot be used after being moved.")
        };
        unsafe {
            f(&mut SignalContext {
                rt: &mut *data.rt,
                bump: &*data.bump,
                sink: data.sink.map(|x| &mut *x),
            })
        }
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

/// Wait until there are no more immediately runnable actions and state updates.
///
/// If [`Runtime::run`] is not being called in the current thread, it will not complete until `Runtime::run` is called.
///
/// # Panics
///
/// Panics if there is no `Runtime` in the current thread.
pub async fn wait_for_update() {
    let phase = Globals::with(|rt| {
        if !rt.is_runtime_exists {
            panic!("There is no `Runtime` in the current thread.");
        }
        rt.phase
    });
    WaitForUpdate {
        phase,
        _not_send: PhantomNotSend::default(),
    }
    .await
}

struct WaitForUpdate {
    phase: usize,
    _not_send: PhantomNotSend,
}
impl Future for WaitForUpdate {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        Globals::with(|rt| rt.poll_wait_for_update(self.phase, cx))
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

    pub fn schedule_with(self, scheduler: &Scheduler) {
        scheduler.schedule(self)
    }
    pub fn schedule(self) {
        DEFAULT_SCHEDULER.with(|s| s.schedule(self))
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

thread_local! {
    static DEFAULT_SCHEDULER: Scheduler = Scheduler::new_default();
}

struct SchedulerData {
    name: String,
    id: Option<usize>,
    tasks: RefCell<VecDeque<Task>>,
}
impl SchedulerData {
    fn cancel(&self) {
        self.tasks.borrow_mut().clear();
    }
}

#[derive(Clone)]
pub struct Scheduler(Rc<SchedulerData>);

impl Scheduler {
    pub fn new(_rt: &Runtime, name: &str) -> Self {
        let id = Globals::with(|g| g.schedulers.insert(Weak::default()));
        let s = Self::new_raw(Some(id), name);
        Globals::with(|g| g.schedulers[id] = Rc::downgrade(&s.0));
        s
    }
    fn new_default() -> Self {
        Self::new_raw(None, "<default>")
    }
    fn new_raw(id: Option<usize>, name: &str) -> Self {
        Self(Rc::new(SchedulerData {
            id,
            name: name.to_string(),
            tasks: RefCell::new(VecDeque::new()),
        }))
    }

    fn schedule(&self, task: Task) {
        let mut tasks = self.0.tasks.borrow_mut();
        let is_wake = !tasks.is_empty();
        tasks.push_back(task);
        if is_wake {
            Globals::with(|g| g.wake());
        }
    }
    fn run(&self, uc: &mut UpdateContext) -> bool {
        let mut handled = false;
        while let Some(task) = self.0.tasks.borrow_mut().pop_front() {
            handled = true;
            task.run(uc);
        }
        handled
    }
}
impl Drop for Scheduler {
    fn drop(&mut self) {
        if let Some(id) = self.0.id {
            Globals::with(|g| g.schedulers.remove(id));
        }
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        DEFAULT_SCHEDULER.with(|s| s.clone())
    }
}
impl std::fmt::Debug for Scheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple(&self.0.name).finish()
    }
}
