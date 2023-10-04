use derive_ex::derive_ex;
use slabmap::SlabMap;
use std::{
    cell::RefCell,
    collections::VecDeque,
    future::Future,
    mem::{replace, transmute},
    pin::{pin, Pin},
    ptr::null_mut,
    rc::{Rc, Weak},
    sync::{Arc, Mutex, MutexGuard},
    task::{Context, Poll, Wake, Waker},
};

pub mod dependency_node;
pub mod dependency_token;

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
}

impl RuntimeGlobal {
    fn with<T>(f: impl FnOnce(&mut RuntimeGlobal) -> T) -> T {
        RG.with(|rg| f(&mut rg.borrow_mut()))
    }
    fn try_with(f: impl FnOnce(&mut RuntimeGlobal)) {
        let _ = RG.try_with(|rg| f(&mut rg.borrow_mut()));
    }

    fn schedule_update(node: Weak<dyn CallUpdate>, slot: usize) {
        Self::with(|rg| rg.tasks_update.push(WeakTaskOf { node, slot }));
    }
    fn schedule_action(action: Action) {
        Self::with(|rg| rg.actions.push_back(action));
    }
}
pub(crate) fn schedule_notify_lazy(node: Weak<dyn BindSink>, slot: usize) {
    RuntimeGlobal::try_with(|rg| rg.tasks_notify.push(WeakTaskOf { node, slot }));
}
pub(crate) fn schedule_update_lazy(node: Weak<dyn CallUpdate>, slot: usize) {
    RuntimeGlobal::try_with(|rg| rg.tasks_update.push(WeakTaskOf { node, slot }));
}

struct RuntimeTasks {
    tasks_flush: Vec<TaskOf<dyn CallFlush>>,
    tasks_update: Vec<TaskOf<dyn CallUpdate>>,
    tasks_discard: Vec<TaskOf<dyn CallDiscard>>,
}
impl RuntimeTasks {
    fn new() -> Self {
        Self {
            tasks_flush: Vec::new(),
            tasks_update: Vec::new(),
            tasks_discard: Vec::new(),
        }
    }
}

pub struct UpdateContext(RuntimeTasks);

impl UpdateContext {
    pub fn oc(&mut self) -> ObsContext {
        ObsContext::new(self, None)
    }
    fn apply_notify(&mut self) {
        RuntimeGlobal::with(|t| {
            for t in t.tasks_unbind.drain(..) {
                t.unbind(self);
            }
            t.wakes.apply(self);
            for t in t.tasks_notify.drain(..) {
                t.call_notify(self);
            }
        });
        while let Some(task) = self.0.tasks_flush.pop() {
            task.call_flush(self);
        }
    }
    fn run_actions(&mut self) {
        while let Some(a) = RuntimeGlobal::with(|t| t.actions.pop_front()) {
            a.call(&mut ActionContext(ObsContext::new(self, None)))
        }
    }
    fn update_all(&mut self, discard: bool) {
        self.run_actions();
        self.apply_notify();
        loop {
            while let Some(task) = self.0.tasks_update.pop() {
                task.call_update(self);
            }
            RuntimeGlobal::with(|t| {
                self.0
                    .tasks_update
                    .extend(t.tasks_update.drain(..).filter_map(|t| t.upgrade()))
            });
            if self.0.tasks_update.is_empty() {
                break;
            }
        }
        if discard {
            while let Some(task) = self.0.tasks_discard.pop() {
                task.call_discard(self);
            }
        }
    }

    pub(crate) fn schedule_flush(&mut self, node: Rc<dyn CallFlush>, slot: usize) {
        self.0.tasks_flush.push(TaskOf { node, slot });
    }
    pub(crate) fn schedule_update(&mut self, node: Rc<dyn CallUpdate>, slot: usize) {
        self.0.tasks_update.push(TaskOf { node, slot });
    }
    pub(crate) fn schedule_discard(&mut self, node: Rc<dyn CallDiscard>, slot: usize) {
        self.0.tasks_discard.push(TaskOf { node, slot });
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Computed {
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

    #[allow(clippy::vtable_address_comparisons)]
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
        compute: impl FnOnce(ComputeContext) -> T,
        uc: &mut UpdateContext,
    ) -> T {
        let mut oc = ObsContext {
            uc,
            sink: Some(ObsContextSink {
                node,
                slot,
                bindings: self,
                bindings_len: 0,
            }),
        };
        let retval = compute(ComputeContext(&mut oc));
        let sink = oc.sink.as_mut().unwrap();
        for b in sink.bindings.0.drain(sink.bindings_len..) {
            b.unbind(oc.uc);
        }
        retval
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
        let Some(sink) = &mut oc.sink else {
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
            replace(&mut sink.bindings.0[sources_index], source_binding).unbind(oc.uc);
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
pub struct Runtime(UpdateContext);

impl Runtime {
    pub fn new() -> Self {
        let Some(tasks) = RG.with(|rg| rg.borrow_mut().tasks_saved.take()) else {
            panic!("Only one `Runtime` can exist in the same thread at the same time.");
        };
        Self(UpdateContext(tasks))
    }

    pub fn ac(&mut self) -> ActionContext {
        ActionContext::new(&mut self.0)
    }
    pub fn uc(&mut self) -> &mut UpdateContext {
        &mut self.0
    }
    pub fn run_actions(&mut self) {
        self.0.run_actions();
    }

    pub fn update(&mut self) {
        self.update_with(true)
    }
    pub fn update_with(&mut self, discard: bool) {
        self.0.update_all(discard);
    }

    pub async fn run<Fut: Future>(&mut self, f: impl FnOnce(RuntimeContext) -> Fut) -> Fut::Output {
        let rt = RuntimeContextSource::new(self);
        let fut = pin!(f(rt.get()));
        let wake = RG.with(|t| RawWake::new(&t.borrow().wakes.requests, None));
        wake.requests().is_wake_main = true;
        RuntimeMain { rt, fut, wake }.await
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        let tasks_saved = Some(replace(&mut self.0 .0, RuntimeTasks::new()));
        let _ = RG.try_with(|rg| rg.borrow_mut().tasks_saved = tasks_saved);
    }
}

struct RuntimeMain<'a, Fut> {
    rt: RuntimeContextSource,
    wake: Arc<RawWake>,
    fut: Pin<&'a mut Fut>,
}

impl<'a, Fut: Future> Future for RuntimeMain<'a, Fut> {
    type Output = Fut::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut is_wake_old = true;
        let mut is_wake = self.wake.requests().is_wake_main;
        while is_wake || is_wake_old {
            if is_wake {
                let p = self.fut.as_mut().poll(cx);
                if p.is_ready() {
                    return p;
                }
                self.wake.requests().is_wake_main = false;
            }
            self.rt.0.run_actions();
            self.rt.0.update();
            is_wake_old = is_wake;
            is_wake = self.wake.requests().is_wake_main;
        }
        self.wake.requests().waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

struct RuntimeContextSource(RuntimeContext);

impl RuntimeContextSource {
    pub fn new(rt: *mut Runtime) -> Self {
        Self(RuntimeContext(Rc::new(RefCell::new(rt))))
    }
    pub fn get(&self) -> RuntimeContext {
        self.0.clone()
    }
}
impl Drop for RuntimeContextSource {
    fn drop(&mut self) {
        self.0.set_null();
    }
}

#[derive(Clone)]
pub struct RuntimeContext(Rc<RefCell<*mut Runtime>>);

impl RuntimeContext {
    pub fn run_actions(&mut self) {
        self.call(|rt| rt.run_actions())
    }
    pub fn update(&mut self) {
        self.call(|rt| rt.update())
    }
    pub fn update_with(&mut self, discard: bool) {
        self.call(|rt| rt.update_with(discard))
    }
    fn set_null(&self) {
        *self.0.borrow_mut() = null_mut();
    }
    fn call<T>(&self, f: impl FnOnce(&mut Runtime) -> T) -> T {
        unsafe {
            let p = self.0.borrow_mut();
            if let Some(rt) = p.as_mut() {
                f(rt)
            } else {
                panic!("`Runtime::run()` was already finished.");
            }
        }
    }
}

pub struct ObsContext<'oc> {
    uc: &'oc mut UpdateContext,
    sink: Option<ObsContextSink<'oc>>,
}

struct ObsContextSink<'oc> {
    node: Weak<dyn BindSink>,
    slot: usize,
    bindings: &'oc mut SourceBindings,
    bindings_len: usize,
}

impl<'oc> ObsContext<'oc> {
    fn new(uc: &'oc mut UpdateContext, sink: Option<ObsContextSink<'oc>>) -> Self {
        ObsContext { uc, sink }
    }
    /// Create a context that does not track dependencies.
    pub fn no_track(&mut self) -> ObsContext {
        self.uc.oc()
    }
    pub fn uc(&mut self) -> &mut UpdateContext {
        self.uc
    }
}

pub struct ComputeContext<'a, 'oc>(&'a mut ObsContext<'oc>);

impl<'a, 'oc> ComputeContext<'a, 'oc> {
    pub fn oc(self) -> &'a mut ObsContext<'oc> {
        self.0
    }
    pub fn oc_with(self, watch_previous_dependencies: bool) -> &'a mut ObsContext<'oc> {
        if watch_previous_dependencies {
            let sink = self.0.sink.as_mut().unwrap();
            debug_assert!(sink.bindings_len == 0);
            sink.bindings_len = sink.bindings.0.len();
        }
        self.0
    }
    pub fn uc(&mut self) -> &mut UpdateContext {
        self.0.uc()
    }
}

pub struct ActionContext<'a>(ObsContext<'a>);

impl<'a> ActionContext<'a> {
    fn new(uc: &'a mut UpdateContext) -> Self {
        Self(ObsContext::new(uc, None))
    }
    pub(crate) fn uc(&mut self) -> &mut UpdateContext {
        self.0.uc
    }

    /// Return [`ObsContext`] to get the new state.
    pub fn oc(&mut self) -> &mut ObsContext<'a> {
        self.uc().apply_notify();
        &mut self.0
    }
}

/// Operation for changing state.
pub struct Action(RawAction);
enum RawAction {
    Box(Box<dyn FnOnce(&mut ActionContext)>),
    Rc(Rc<dyn Fn(&mut ActionContext)>),
}
impl Action {
    pub fn new(f: impl FnOnce(&mut ActionContext) + 'static) -> Self {
        Self(RawAction::Box(Box::new(f)))
    }
    pub fn call(self, ac: &mut ActionContext) {
        match self.0 {
            RawAction::Box(f) => f(ac),
            RawAction::Rc(f) => f(ac),
        }
    }

    /// Perform this action after [`ActionContext`] is available.
    pub fn schedule(self) {
        RuntimeGlobal::schedule_action(self)
    }
}
impl<T: FnOnce(&mut ActionContext) + 'static> From<T> for Action {
    fn from(f: T) -> Self {
        Action::new(f)
    }
}

/// Shareable [`Action`].
pub struct RcAction(Rc<dyn Fn(&mut ActionContext)>);

impl RcAction {
    pub fn new(f: impl Fn(&mut ActionContext) + 'static) -> Self {
        RcAction(Rc::new(f))
    }

    /// Perform this action after [`ActionContext`] is available.
    pub fn schedule(&self) {
        Action::schedule(self.into())
    }
}
impl From<&RcAction> for Action {
    fn from(rc: &RcAction) -> Self {
        Self(RawAction::Rc(rc.0.clone()))
    }
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
    tasks: SlabMap<WeakTaskOf<dyn BindSink>>,
    requests: WakeRequests,
}

impl WakeTable {
    fn insert(&mut self, task: WeakTaskOf<dyn BindSink>) -> Arc<RawWake> {
        RawWake::new(&self.requests, Some(self.tasks.insert(task)))
    }

    fn apply(&mut self, uc: &mut UpdateContext) {
        let mut requests = self.requests.0.lock().unwrap();
        for key in requests.drops.drain(..) {
            self.tasks.remove(key);
        }
        for key in requests.wakes.drain(..) {
            if let Some(task) = self.tasks.get(key) {
                task.call_notify(uc);
            }
        }
        requests.waker = None;
    }
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
        if let Some(waker) = requests.waker.take() {
            waker.wake();
        }
    }
}
impl Drop for RawWake {
    fn drop(&mut self) {
        if let Some(key) = self.key {
            self.requests.0.lock().unwrap().drops.push(key);
        }
    }
}
pub(crate) struct DependencyWaker(Arc<RawWake>);

impl DependencyWaker {
    pub fn new(node: Weak<impl BindSink>, slot: usize) -> Self {
        let node = node;
        Self(RuntimeGlobal::with(|t| {
            t.wakes.insert(WeakTaskOf { node, slot })
        }))
    }
    pub fn as_waker(&self) -> Waker {
        self.0.clone().into()
    }
}

#[derive_ex(Default)]
#[default(Self::new())]
pub(crate) struct AsyncObsContextSource(Rc<RefCell<*mut ObsContext<'static>>>);

impl AsyncObsContextSource {
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(null_mut())))
    }
    pub fn set<T>(&self, oc: &mut ObsContext, f: impl FnOnce() -> T) -> T {
        let p = unsafe { transmute(oc) };
        assert!(self.0.borrow().is_null());
        *self.0.borrow_mut() = p;
        let ret = f();
        assert!(*self.0.borrow() == p);
        *self.0.borrow_mut() = null_mut();
        ret
    }
    pub fn context(&self) -> AsyncObsContext {
        AsyncObsContext(self.0.clone())
    }
}

pub struct AsyncObsContext(Rc<RefCell<*mut ObsContext<'static>>>);

impl AsyncObsContext {
    pub fn get<T>(&mut self, f: impl FnOnce(&mut ObsContext) -> T) -> T {
        let mut b = self.0.borrow_mut();
        assert!(!b.is_null());
        unsafe { f(&mut **b) }
    }
}
