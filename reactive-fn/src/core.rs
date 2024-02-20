use bumpalo::Bump;
use derive_ex::derive_ex;
use slabmap::SlabMap;
use std::{
    any::Any,
    cell::RefCell,
    collections::VecDeque,
    future::Future,
    mem::{replace, take},
    pin::{pin, Pin},
    ptr::null_mut,
    rc::{Rc, Weak},
    sync::{Arc, Mutex, MutexGuard},
    task::{Context, Poll, Wake, Waker},
};

use crate::utils::PhantomNotSend;

mod obs_ref;

pub use obs_ref::*;

thread_local! {
    static RG : RefCell<RuntimeGlobal> = Default::default();
}

#[derive_ex(Default)]
struct RuntimeGlobal {
    tasks_update: Vec<WeakTaskOf<dyn CallUpdate>>,
    tasks_notify: Vec<WeakTaskOf<dyn BindSink>>,
    tasks_unbind: Vec<UnbindTask>,
    #[default(Some(RuntimeTasks::new()))]
    tasks_saved: Option<RuntimeTasks>,
    actions: VecDeque<Action>,
    wakes: WakeTable,
    wait_for_update_wakers: Vec<Waker>,
    phase: usize,
}

impl RuntimeGlobal {
    fn with<T>(f: impl FnOnce(&mut RuntimeGlobal) -> T) -> T {
        RG.with(|rg| f(&mut rg.borrow_mut()))
    }
    fn try_with(f: impl FnOnce(&mut RuntimeGlobal)) {
        let _ = RG.try_with(|rg| f(&mut rg.borrow_mut()));
    }

    fn push_action(&mut self, action: Action) {
        self.actions.push_back(action);
        self.wake();
    }
    fn push_notify(&mut self, node: Weak<dyn BindSink>, slot: usize) {
        self.tasks_notify.push(WeakTaskOf { node, slot });
        self.wake();
    }
    fn push_update(&mut self, node: Weak<dyn CallUpdate>, slot: usize) {
        self.tasks_update.push(WeakTaskOf { node, slot });
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
                    WakeTask::Notify { sink, slot } => {
                        self.tasks_notify.push(WeakTaskOf {
                            node: sink.clone(),
                            slot: *slot,
                        });
                    }
                    WakeTask::AsyncAction(action) => self.actions.push_back(action.to_action()),
                }
            }
        }
        requests.waker = None;
        take(&mut requests.is_wake_main)
    }
    fn poll_wait_for_update(&mut self, phase: usize, cx: &mut Context) -> Poll<()> {
        if !self.is_runtime_exists() || phase != self.phase {
            return Poll::Ready(());
        }
        self.wait_for_update_wakers.push(cx.waker().clone());
        self.wake();
        Poll::Pending
    }
    fn wake_wait_for_update(&mut self) -> bool {
        let mut is_used = false;
        self.phase += 1;
        while let Some(waker) = self.wait_for_update_wakers.pop() {
            is_used = true;
            waker.wake();
        }
        is_used
    }
    fn is_runtime_exists(&self) -> bool {
        self.tasks_saved.is_none()
    }
    fn wake(&mut self) {
        self.wakes.requests.0.lock().unwrap().wake();
    }
}
pub fn schedule_notify(node: Weak<dyn BindSink>, slot: usize) {
    RuntimeGlobal::try_with(|rg| rg.push_notify(node, slot));
}
pub(crate) fn schedule_update(node: Weak<dyn CallUpdate>, slot: usize) {
    RuntimeGlobal::try_with(|rg| rg.push_update(node, slot));
}

struct RuntimeTasks {
    tasks_flush: Vec<TaskOf<dyn CallFlush>>,
    tasks_update: Vec<TaskOf<dyn CallUpdate>>,
    tasks_discard: Vec<TaskOf<dyn CallDiscard>>,
    async_actions: SlabMap<Rc<AsyncAction>>,
}
impl RuntimeTasks {
    fn new() -> Self {
        Self {
            tasks_flush: Vec::new(),
            tasks_update: Vec::new(),
            tasks_discard: Vec::new(),
            async_actions: SlabMap::new(),
        }
    }
}

pub struct UpdateContext<'a> {
    tasks: &'a mut RuntimeTasks,
    bump: &'a Bump,
    sink: Option<ObsContextSink>,
}

impl<'a> UpdateContext<'a> {
    fn new(rt: &'a mut Runtime) -> Self {
        Self {
            tasks: &mut rt.tasks,
            bump: &rt.bump,
            sink: None,
        }
    }

    fn apply_notify(&mut self) -> bool {
        let mut is_used = false;
        RuntimeGlobal::with(|t| {
            for t in t.tasks_unbind.drain(..) {
                t.unbind(self);
                is_used = true;
            }
            for t in t.tasks_notify.drain(..) {
                t.call_notify(self);
                is_used = true;
            }
        });
        while let Some(task) = self.tasks.tasks_flush.pop() {
            task.call_flush(self);
            is_used = true;
        }
        is_used
    }
    pub fn oc_with<T>(&mut self, f: impl FnOnce(&mut ObsContext) -> T) -> T {
        f(ObsContextGuard::new(self, None).oc())
    }
    pub fn alloc<T>(&mut self, value: T) -> &'a mut T {
        self.bump.alloc(value)
    }

    pub(crate) fn schedule_flush(&mut self, node: Rc<dyn CallFlush>, slot: usize) {
        self.tasks.tasks_flush.push(TaskOf { node, slot });
    }
    pub(crate) fn schedule_update(&mut self, node: Rc<dyn CallUpdate>, slot: usize) {
        self.tasks.tasks_update.push(TaskOf { node, slot });
    }
    pub(crate) fn schedule_discard(&mut self, node: Rc<dyn CallDiscard>, slot: usize) {
        self.tasks.tasks_discard.push(TaskOf { node, slot });
    }
}

struct ObsContextGuard<'a, 'oc> {
    uc: &'a mut UpdateContext<'oc>,
    sink_old: Option<Option<ObsContextSink>>,
}

impl<'a, 'oc> ObsContextGuard<'a, 'oc> {
    fn new(uc: &'a mut UpdateContext<'oc>, sink: Option<ObsContextSink>) -> Self {
        let sink_old = Some(replace(&mut uc.sink, sink));
        Self { uc, sink_old }
    }
    fn oc(&mut self) -> &mut ObsContext<'oc> {
        ObsContext::new(self.uc)
    }
    fn finish(mut self) -> Option<ObsContextSink> {
        replace(&mut self.uc.sink, self.sink_old.take().unwrap())
    }
}
impl<'oc> Drop for ObsContextGuard<'_, 'oc> {
    fn drop(&mut self) {
        if let Some(sink) = self.sink_old.take() {
            self.uc.sink = sink;
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Computed {
    /// Not calculated.
    ///
    /// Distinguished from `Outdated` to manage the call to `discard`.
    #[default]
    None,
    Outdated,
    MayBeOutdated,
    UpToDate,
}

impl Computed {
    pub fn is_may_up_to_date(&self) -> bool {
        match self {
            Computed::None | Computed::Outdated => false,
            Computed::UpToDate | Computed::MayBeOutdated => true,
        }
    }
    pub fn modify(&mut self, is_modified: bool) -> bool {
        match (is_modified, *self) {
            (true, Computed::MayBeOutdated | Computed::UpToDate) => {
                *self = Computed::Outdated;
                true
            }
            (false, Computed::UpToDate) => {
                *self = Computed::MayBeOutdated;
                true
            }
            _ => false,
        }
    }
}

struct SourceBinding {
    source: Rc<dyn BindSource>,
    slot: usize,
    key: usize,
}

impl SourceBinding {
    fn flush(&self, uc: &mut UpdateContext) -> bool {
        self.source.clone().flush(self.slot, uc)
    }

    fn is_same(&self, node: &Rc<dyn BindSource>, slot: usize) -> bool {
        Rc::ptr_eq(&self.source, node) && self.slot == slot
    }

    fn unbind(self, uc: &mut UpdateContext) {
        self.source.unbind(self.slot, self.key, uc)
    }
    fn to_unbind_task(&self) -> UnbindTask {
        UnbindTask {
            node: Rc::downgrade(&self.source),
            slot: self.slot,
            key: self.key,
        }
    }
}

#[derive_ex(Default)]
pub struct SourceBindings(Vec<SourceBinding>);

impl SourceBindings {
    pub fn new() -> Self {
        Self(Vec::new())
    }
    pub fn flush(&self, uc: &mut UpdateContext) -> bool {
        let mut is_modified = false;
        for source in &self.0 {
            if source.flush(uc) {
                is_modified = true;
                break;
            }
        }
        is_modified
    }
    pub fn compute<T>(
        &mut self,
        node: Weak<dyn BindSink>,
        slot: usize,
        f: impl FnOnce(&mut ObsContext) -> T,
        uc: &mut UpdateContext,
    ) -> T {
        let sink = ObsContextSink {
            node,
            slot,
            bindings: take(self),
            bindings_len: self.0.len(),
        };
        let mut oc = ObsContextGuard::new(uc, Some(sink));
        let ret = f(oc.oc());
        let sink = oc.finish().unwrap();
        *self = sink.bindings;
        for b in self.0.drain(sink.bindings_len..) {
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
        RuntimeGlobal::try_with(|t| {
            t.tasks_unbind
                .extend(self.0.iter().map(|b| b.to_unbind_task()))
        });
    }
}
struct UnbindTask {
    node: Weak<dyn BindSource>,
    slot: usize,
    key: usize,
}

impl UnbindTask {
    fn unbind(self, uc: &mut UpdateContext) {
        if let Some(node) = self.node.upgrade() {
            SourceBinding {
                source: node,
                slot: self.slot,
                key: self.key,
            }
            .unbind(uc)
        }
    }
}

#[derive(Default)]
pub struct SinkBindings(SlabMap<SinkBinding>);

impl SinkBindings {
    pub fn new() -> Self {
        Self(SlabMap::new())
    }
    pub fn watch(&mut self, this: Rc<dyn BindSource>, this_slot: usize, oc: &mut ObsContext) {
        let Some(sink) = &mut oc.0.sink else {
            return;
        };
        let sources_index = sink.bindings_len;
        sink.bindings_len += 1;
        if let Some(source_old) = sink.bindings.0.get(sources_index) {
            if source_old.is_same(&this, this_slot) {
                return;
            }
        }
        let sink_binding = SinkBinding {
            node: sink.node.clone(),
            slot: sink.slot,
        };
        let key = self.0.insert(sink_binding);
        let source_binding = SourceBinding {
            source: this,
            slot: this_slot,
            key,
        };
        if sources_index < sink.bindings.0.len() {
            replace(&mut sink.bindings.0[sources_index], source_binding).unbind(oc.uc());
        } else {
            sink.bindings.0.push(source_binding);
        }
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn unbind(&mut self, key: usize) {
        self.0.remove(key);
    }
    pub fn notify(&mut self, is_modified: bool, uc: &mut UpdateContext) {
        self.0.optimize();
        for binding in self.0.values() {
            binding.notify(is_modified, uc);
        }
    }
}

struct SinkBinding {
    node: Weak<dyn BindSink>,
    slot: usize,
}

impl SinkBinding {
    fn notify(&self, is_modified: bool, uc: &mut UpdateContext) {
        if let Some(node) = self.node.upgrade() {
            node.notify(self.slot, is_modified, uc)
        }
    }
}

#[derive_ex(Default)]
#[default(Self::new())]
pub struct Runtime {
    tasks: RuntimeTasks,
    bump: Bump,
}

impl Runtime {
    pub fn new() -> Self {
        let Some(tasks) = RG.with(|rg| rg.borrow_mut().tasks_saved.take()) else {
            panic!("Only one `Runtime` can exist in the same thread at the same time.");
        };
        let bump = Bump::new();
        Self { tasks, bump }
    }

    pub fn ac(&mut self) -> ActionContext {
        ActionContext(self)
    }
    pub fn uc(&mut self) -> UpdateContext {
        self.bump.reset();
        UpdateContext {
            tasks: &mut self.tasks,
            bump: &mut self.bump,
            sink: None,
        }
    }
    pub fn oc(&mut self) -> ObsContext {
        ObsContext(self.uc())
    }

    pub fn run_actions(&mut self) -> bool {
        let mut is_used = false;
        while let Some(a) = RuntimeGlobal::with(|t| t.actions.pop_front()) {
            a.call(&mut self.ac());
            is_used = true;
        }
        is_used
    }

    pub fn update(&mut self) -> bool {
        self.update_with(true)
    }
    pub fn update_with(&mut self, discard: bool) -> bool {
        let mut is_used = false;
        is_used |= self.run_actions();
        is_used |= self.uc().apply_notify();
        loop {
            while let Some(task) = self.tasks.tasks_update.pop() {
                task.call_update(&mut self.uc());
                is_used = true;
            }
            RuntimeGlobal::with(|t| {
                self.tasks
                    .tasks_update
                    .extend(t.tasks_update.drain(..).filter_map(|t| t.upgrade()))
            });
            if self.tasks.tasks_update.is_empty() {
                break;
            }
        }
        if discard {
            while let Some(task) = self.tasks.tasks_discard.pop() {
                task.call_discard(&mut self.uc());
                is_used = true;
            }
        }
        is_used
    }

    pub async fn run<Fut: Future>(&mut self, f: impl FnOnce(RuntimeContext) -> Fut) -> Fut::Output {
        let rts = RuntimeContextSource::new();
        let rt = rts.context();
        let fut = pin!(rts.apply(self, move || f(rt)));
        let wake = RG.with(|t| RawWake::new(&t.borrow().wakes.requests, None));
        wake.requests().is_wake_main = true;
        RuntimeMain {
            rt: self,
            rts,
            fut,
            wake,
        }
        .await
    }
    fn drop_actions(&mut self) {
        let mut actions = Vec::new();
        loop {
            actions.clear();
            actions.extend(self.tasks.async_actions.values().cloned());
            if actions.is_empty() {
                break;
            }
            for action in &actions {
                action.drop_action(&mut self.ac());
            }
        }
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.drop_actions();
        let tasks_saved = Some(replace(&mut self.tasks, RuntimeTasks::new()));
        let _ = RG.try_with(|rg| rg.borrow_mut().tasks_saved = tasks_saved);
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
            if RuntimeGlobal::with(|rt| rt.apply_wake()) {
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
            if this.rt.update() {
                continue;
            }
            if RuntimeGlobal::with(|rt| rt.wake_wait_for_update()) {
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
    pub fn run_actions(&mut self) -> bool {
        self.call(|rt| rt.run_actions())
    }
    pub fn update(&mut self) -> bool {
        self.call(|rt| rt.update())
    }
    pub fn update_with(&mut self, discard: bool) -> bool {
        self.call(|rt| rt.update_with(discard))
    }
    fn call<T>(&self, f: impl FnOnce(&mut Runtime) -> T) -> T {
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

#[repr(transparent)]
pub struct ObsContext<'oc>(UpdateContext<'oc>);

impl<'oc> ObsContext<'oc> {
    fn new<'a>(uc: &'a mut UpdateContext<'oc>) -> &'a mut Self {
        unsafe { &mut *(uc as *mut UpdateContext<'oc> as *mut Self) }
    }

    pub fn reset(&mut self) -> &mut Self {
        if let Some(sink) = &mut self.0.sink {
            sink.bindings_len = 0;
        }
        self
    }

    /// Create a context that does not track dependencies.
    pub fn untrack<T>(&mut self, f: impl Fn(&mut ObsContext<'oc>) -> T) -> T {
        f(ObsContextGuard::new(&mut self.0, None).oc())
    }
    pub fn uc(&mut self) -> &mut UpdateContext<'oc> {
        &mut self.0
    }

    fn bump(&self) -> &'oc Bump {
        self.0.bump
    }
}

struct ObsContextSink {
    node: Weak<dyn BindSink>,
    slot: usize,
    bindings: SourceBindings,
    bindings_len: usize,
}

pub struct ActionContext<'ac>(&'ac mut Runtime);

impl<'ac> ActionContext<'ac> {
    pub fn uc(&mut self) -> UpdateContext {
        self.0.bump.reset();
        UpdateContext::new(self.0)
    }

    /// Return [`ObsContext`] to get the new state.
    pub fn oc(&mut self) -> ObsContext {
        let mut uc = self.uc();
        uc.apply_notify();
        ObsContext(uc)
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
pub fn spawn_action_from_rc<T: Any>(
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
            f: Box::new(move |this, oc| f(this.downcast().unwrap(), oc)),
        }
    }

    pub fn call(self, ac: &mut ActionContext) {
        match self {
            Action::Box(f) => f(ac),
            Action::Rc { this, f } => f(this, ac),
        }
    }
    pub fn schedule(self) {
        RuntimeGlobal::try_with(|rg| rg.push_action(self));
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
        let id = ac.0.tasks.async_actions.insert(action.clone());
        *action.data.borrow_mut() = Some(AsyncActionData {
            id,
            waker: RuntimeWaker::from_async_action(action.clone()),
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
            ac.0.tasks.async_actions.remove(id_remove);
        }
    }

    fn drop_action(self: &Rc<Self>, ac: &mut ActionContext) {
        self.call(ac, |data| Some(data.take()?.id))
    }
    fn next(self: Rc<Self>, ac: &mut ActionContext) {
        self.call(ac, |data| {
            let d = data.as_mut()?;
            let waker = d.waker.as_waker();
            let mut cx = Context::from_waker(&waker);
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
    waker: RuntimeWaker,
    id: usize,
}

pub trait BindSink: 'static {
    fn notify(self: Rc<Self>, slot: usize, is_modified: bool, uc: &mut UpdateContext);
}

pub trait BindSource: 'static {
    /// Determine whether the status is up-to-date or not.
    ///
    /// Return true if the status changes from "Undetermined" to "out-of-date".
    fn flush(self: Rc<Self>, slot: usize, uc: &mut UpdateContext) -> bool;
    fn unbind(self: Rc<Self>, slot: usize, key: usize, uc: &mut UpdateContext);
}

pub(crate) trait CallFlush: 'static {
    fn call_flush(self: Rc<Self>, slot: usize, uc: &mut UpdateContext);
}
pub(crate) trait CallUpdate: 'static {
    fn call_update(self: Rc<Self>, slot: usize, uc: &mut UpdateContext);
}
pub(crate) trait CallDiscard: 'static {
    fn call_discard(self: Rc<Self>, slot: usize, uc: &mut UpdateContext);
}

struct TaskOf<T: ?Sized> {
    node: Rc<T>,
    slot: usize,
}
impl TaskOf<dyn CallFlush> {
    fn call_flush(self, uc: &mut UpdateContext) {
        self.node.call_flush(self.slot, uc)
    }
}
impl TaskOf<dyn CallUpdate> {
    fn call_update(self, uc: &mut UpdateContext) {
        self.node.call_update(self.slot, uc)
    }
}
impl TaskOf<dyn CallDiscard> {
    fn call_discard(self, uc: &mut UpdateContext) {
        self.node.call_discard(self.slot, uc)
    }
}

struct WeakTaskOf<T: ?Sized> {
    node: Weak<T>,
    slot: usize,
}
impl<T: ?Sized> WeakTaskOf<T> {
    fn upgrade(self) -> Option<TaskOf<T>> {
        Some(TaskOf {
            node: self.node.upgrade()?,
            slot: self.slot,
        })
    }
}
impl WeakTaskOf<dyn BindSink> {
    fn call_notify(&self, uc: &mut UpdateContext) {
        if let Some(node) = self.node.upgrade() {
            node.notify(self.slot, true, uc)
        }
    }
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
    Notify {
        sink: Weak<dyn BindSink>,
        slot: usize,
    },
    AsyncAction(Rc<AsyncAction>),
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
pub(crate) struct RuntimeWaker(Arc<RawWake>);

impl RuntimeWaker {
    pub fn from_sink(sink: Weak<impl BindSink>, slot: usize) -> Self {
        Self::new(WakeTask::Notify { sink, slot })
    }
    fn from_async_action(action: Rc<AsyncAction>) -> Self {
        Self::new(WakeTask::AsyncAction(action))
    }
    fn new(task: WakeTask) -> Self {
        Self(RuntimeGlobal::with(|t| t.wakes.insert(task)))
    }

    pub fn as_waker(&self) -> Waker {
        self.0.clone().into()
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
struct AsyncObsContextData {
    tasks: *mut RuntimeTasks,
    bump: *const Bump,
    sink: *mut Option<ObsContextSink>,
}
impl AsyncObsContextData {
    fn new(oc: &mut ObsContext) -> Self {
        Self {
            tasks: oc.0.tasks,
            bump: oc.0.bump,
            sink: &mut oc.0.sink,
        }
    }
}

pub(crate) struct AsyncObsContextSource(Rc<RefCell<Option<AsyncObsContextData>>>);

impl AsyncObsContextSource {
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(None)))
    }
    pub fn call<T>(&self, oc: &mut ObsContext, f: impl FnOnce() -> T) -> T {
        let data = AsyncObsContextData::new(oc);
        assert!(self.0.borrow().is_none());
        *self.0.borrow_mut() = Some(data);
        let ret = f();
        assert!(*self.0.borrow() == Some(data));
        *self.0.borrow_mut() = None;
        ret
    }
    pub fn context(&self) -> AsyncObsContext {
        AsyncObsContext(self.0.clone())
    }
}

pub struct AsyncObsContext(Rc<RefCell<Option<AsyncObsContextData>>>);

impl AsyncObsContext {
    pub fn get<T>(&mut self, f: impl FnOnce(&mut ObsContext) -> T) -> T {
        let mut data = self.0.borrow_mut();
        let Some(data) = data.as_mut() else {
            panic!("`AsyncObsContext` cannot be used after being moved.")
        };
        unsafe {
            let mut uc = UpdateContext {
                tasks: &mut *data.tasks,
                bump: &*data.bump,
                sink: None,
            };
            let mut oc = ObsContextGuard::new(&mut uc, (*data.sink).take());
            let ret = f(oc.oc());
            *data.sink = oc.finish();
            ret
        }
    }
}

struct AsyncActionContextSource(Rc<RefCell<*mut Runtime>>);

impl AsyncActionContextSource {
    fn new() -> Self {
        Self(Rc::new(RefCell::new(null_mut())))
    }
    fn call<T>(&self, ac: &mut ActionContext, f: impl FnOnce() -> T) -> T {
        let p: *mut Runtime = ac.0;
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
        unsafe { f(&mut (**b).ac()) }
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
    let phase = RuntimeGlobal::with(|rt| {
        if !rt.is_runtime_exists() {
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
        RuntimeGlobal::with(|rt| rt.poll_wait_for_update(self.phase, cx))
    }
}
