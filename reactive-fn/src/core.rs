use derive_ex::derive_ex;
use slabmap::SlabMap;
use std::{
    cell::RefCell,
    collections::VecDeque,
    mem::{replace, transmute},
    ptr::null_mut,
    rc::{Rc, Weak},
    sync::{Arc, Mutex},
    task::{Wake, Waker},
};

pub mod dependency_node;
pub mod dependency_token;

thread_local! {
    static TASKS : RefCell<LazyTasks> = Default::default();
    static RT : RefCell<Runtime> = RefCell::new(Runtime::new());
}

#[derive(Default)]
struct LazyTasks {
    tasks_update: Vec<WeakTaskOf<dyn CallUpdate>>,
    tasks_notify: Vec<WeakTaskOf<dyn BindSink>>,
    tasks_unbind: Vec<UnbindTask>,
    actions: VecDeque<Action>,
    wakes: WakeTable,
}

impl LazyTasks {
    fn with<T>(f: impl FnOnce(&mut LazyTasks) -> T) -> T {
        TASKS.with(|t| f(&mut t.borrow_mut()))
    }
    fn try_with(f: impl FnOnce(&mut LazyTasks)) {
        let _ = TASKS.try_with(|t| f(&mut t.borrow_mut()));
    }

    fn schedule_update(node: Weak<dyn CallUpdate>, param: usize) {
        Self::with(|t| t.tasks_update.push(WeakTaskOf { node, param }));
    }
    fn schedule_action(action: Action) {
        Self::with(|t| t.actions.push_back(action));
    }
}

pub(crate) struct Runtime {
    uc: UpdateContext,
}

impl Runtime {
    fn new() -> Self {
        Self {
            uc: UpdateContext(RawRuntime::new()),
        }
    }
    pub fn schedule_notify_lazy(node: Weak<dyn BindSink>, param: usize) {
        LazyTasks::try_with(|t| t.tasks_notify.push(WeakTaskOf { node, param }));
    }
    pub fn schedule_update_lazy(node: Weak<dyn CallUpdate>, param: usize) {
        LazyTasks::try_with(|t| t.tasks_update.push(WeakTaskOf { node, param }));
    }
}
struct RawRuntime {
    tasks_flush: Vec<TaskOf<dyn CallFlush>>,
    tasks_update: Vec<TaskOf<dyn CallUpdate>>,
    tasks_discard: Vec<TaskOf<dyn CallDiscard>>,
}
impl RawRuntime {
    fn new() -> Self {
        Self {
            tasks_flush: Vec::new(),
            tasks_update: Vec::new(),
            tasks_discard: Vec::new(),
        }
    }
}

pub struct UpdateContext(RawRuntime);

impl UpdateContext {
    fn apply_notify(&mut self) {
        LazyTasks::with(|t| {
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
        while let Some(a) = LazyTasks::with(|t| t.actions.pop_front()) {
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
            LazyTasks::with(|t| {
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

    pub(crate) fn schedule_flush(&mut self, node: Rc<dyn CallFlush>, param: usize) {
        self.0.tasks_flush.push(TaskOf { node, param });
    }
    pub(crate) fn schedule_update(&mut self, node: Rc<dyn CallUpdate>, param: usize) {
        self.0.tasks_update.push(TaskOf { node, param });
    }
    pub(crate) fn schedule_discard(&mut self, node: Rc<dyn CallDiscard>, param: usize) {
        self.0.tasks_discard.push(TaskOf { node, param });
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
    node: Rc<dyn BindSource>,
    param: usize,
    key: usize,
}

impl SourceBinding {
    fn flush(&self, uc: &mut UpdateContext) -> bool {
        self.node.clone().flush(self.param, uc)
    }

    #[allow(clippy::vtable_address_comparisons)]
    fn is_same(&self, node: &Rc<dyn BindSource>, param: usize) -> bool {
        Rc::ptr_eq(&self.node, node) && self.param == param
    }

    fn unbind(self, uc: &mut UpdateContext) {
        self.node.unbind(self.param, self.key, uc)
    }
    fn to_unbind_task(&self) -> UnbindTask {
        UnbindTask {
            node: Rc::downgrade(&self.node),
            param: self.param,
            key: self.key,
        }
    }
}
pub(crate) struct SourceBindings(Vec<SourceBinding>);

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
        param: usize,
        compute: impl FnOnce(&mut ComputeContext) -> T,
        uc: &mut UpdateContext,
    ) -> T {
        let mut cc = ComputeContext(ObsContext {
            uc,
            sink: Some(ObsContextSink {
                node,
                param,
                bindings: self,
                bindings_len: 0,
            }),
        });
        let retval = compute(&mut cc);
        cc.finish();
        retval
    }
}
impl Drop for SourceBindings {
    fn drop(&mut self) {
        LazyTasks::try_with(|t| {
            t.tasks_unbind
                .extend(self.0.iter().map(|b| b.to_unbind_task()))
        });
    }
}
struct UnbindTask {
    node: Weak<dyn BindSource>,
    param: usize,
    key: usize,
}

impl UnbindTask {
    fn unbind(self, uc: &mut UpdateContext) {
        if let Some(node) = self.node.upgrade() {
            SourceBinding {
                node,
                param: self.param,
                key: self.key,
            }
            .unbind(uc)
        }
    }
}

#[derive(Default)]
pub(crate) struct SinkBindings(SlabMap<SinkBinding>);

impl SinkBindings {
    pub fn new() -> Self {
        Self(SlabMap::new())
    }
    pub fn watch(&mut self, this: Rc<dyn BindSource>, this_param: usize, oc: &mut ObsContext) {
        let Some(sink) = &mut oc.sink else { return; };
        let sources_index = sink.bindings_len;
        sink.bindings_len += 1;
        if let Some(source_old) = sink.bindings.0.get(sources_index) {
            if source_old.is_same(&this, this_param) {
                return;
            }
        }
        let sink_binding = SinkBinding {
            node: sink.node.clone(),
            param: sink.param,
        };
        let key = self.0.insert(sink_binding);
        let source_binding = SourceBinding {
            node: this,
            param: this_param,
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
        for sink in self.0.values() {
            sink.notify(is_modified, uc);
        }
    }
}

struct SinkBinding {
    node: Weak<dyn BindSink>,
    param: usize,
}

impl SinkBinding {
    fn notify(&self, is_modified: bool, uc: &mut UpdateContext) {
        if let Some(node) = self.node.upgrade() {
            node.notify(self.param, is_modified, uc)
        }
    }
}

pub struct DependencyContext<'a>(&'a mut UpdateContext);

impl<'a> DependencyContext<'a> {
    fn new(uc: &'a mut UpdateContext) -> Self {
        Self(uc)
    }
    pub fn ac(&mut self) -> ActionContext {
        ActionContext::new(self.0)
    }
    pub fn uc(&mut self) -> &mut UpdateContext {
        self.0
    }
    pub fn schedule_action(&mut self, action: impl Into<Action>) {
        let action: Action = action.into();
        action.schedule();
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

    /// Get the `DependencyContext` associated with the current thread.
    ///
    /// # Panics
    ///
    /// Panic if `DependencyContext` already used in the current thread.
    pub fn with<T>(f: impl FnOnce(&mut DependencyContext) -> T) -> T {
        RT.with(|uc| {
            if let Ok(mut rt) = uc.try_borrow_mut() {
                f(&mut DependencyContext::new(&mut rt.uc))
            } else {
                panic!("`DependencyGraph` already used.")
            }
        })
    }
}

pub struct ObsContext<'oc> {
    uc: &'oc mut UpdateContext,
    sink: Option<ObsContextSink<'oc>>,
}

struct ObsContextSink<'oc> {
    node: Weak<dyn BindSink>,
    param: usize,
    bindings: &'oc mut SourceBindings,
    bindings_len: usize,
}

impl<'oc> ObsContext<'oc> {
    fn new(uc: &'oc mut UpdateContext, sink: Option<ObsContextSink<'oc>>) -> Self {
        ObsContext { uc, sink }
    }
    pub fn schedule_action(&mut self, action: impl Into<Action>) {
        let action: Action = action.into();
        action.schedule();
    }
    pub fn uc(&mut self) -> &mut UpdateContext {
        self.uc
    }
    pub fn nul(&mut self) -> ObsContext {
        ObsContext::new(self.uc, None)
    }
}

pub struct ComputeContext<'oc>(ObsContext<'oc>);

impl<'oc> ComputeContext<'oc> {
    pub fn oc(&mut self) -> &mut ObsContext<'oc> {
        &mut self.0
    }
    pub fn uc(&mut self) -> &mut UpdateContext {
        self.oc().uc()
    }
    pub fn watch_previous_dependencies(&mut self) {
        let sink = self.0.sink.as_mut().unwrap();
        assert!(
            sink.bindings_len == 0,
            "`watch_previous_dependencies` must be called before watch any sources."
        );
        sink.bindings_len = sink.bindings.0.len();
    }
    fn finish(&mut self) {
        let sink = self.0.sink.as_mut().unwrap();
        for b in sink.bindings.0.drain(sink.bindings_len..) {
            b.unbind(self.0.uc);
        }
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
        LazyTasks::schedule_action(self)
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

pub(crate) trait BindSink: 'static {
    fn notify(self: Rc<Self>, param: usize, is_modified: bool, uc: &mut UpdateContext);
}

pub(crate) trait BindSource: 'static {
    fn flush(self: Rc<Self>, param: usize, uc: &mut UpdateContext) -> bool;
    fn unbind(self: Rc<Self>, param: usize, key: usize, uc: &mut UpdateContext);
}

pub(crate) trait CallFlush: 'static {
    fn call_flush(self: Rc<Self>, param: usize, uc: &mut UpdateContext);
}
pub(crate) trait CallUpdate: 'static {
    fn call_update(self: Rc<Self>, param: usize, uc: &mut UpdateContext);
}
pub(crate) trait CallDiscard: 'static {
    fn call_discard(self: Rc<Self>, param: usize, uc: &mut UpdateContext);
}

struct TaskOf<T: ?Sized> {
    node: Rc<T>,
    param: usize,
}
impl TaskOf<dyn CallFlush> {
    fn call_flush(self, uc: &mut UpdateContext) {
        self.node.call_flush(self.param, uc)
    }
}
impl TaskOf<dyn CallUpdate> {
    fn call_update(self, uc: &mut UpdateContext) {
        self.node.call_update(self.param, uc)
    }
}
impl TaskOf<dyn CallDiscard> {
    fn call_discard(self, uc: &mut UpdateContext) {
        self.node.call_discard(self.param, uc)
    }
}

struct WeakTaskOf<T: ?Sized> {
    node: Weak<T>,
    param: usize,
}
impl<T: ?Sized> WeakTaskOf<T> {
    fn upgrade(self) -> Option<TaskOf<T>> {
        Some(TaskOf {
            node: self.node.upgrade()?,
            param: self.param,
        })
    }
}
impl WeakTaskOf<dyn BindSink> {
    fn call_notify(&self, uc: &mut UpdateContext) {
        if let Some(node) = self.node.upgrade() {
            node.notify(self.param, true, uc)
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
        let key = self.tasks.insert(task);
        Arc::new(RawWake {
            key,
            requests: self.requests.clone(),
        })
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
    waker: Option<Waker>,
}

struct RawWake {
    key: usize,
    requests: WakeRequests,
}
impl Wake for RawWake {
    fn wake(self: Arc<Self>) {
        let mut requests = self.requests.0.lock().unwrap();
        requests.wakes.push(self.key);
        if let Some(waker) = requests.waker.take() {
            waker.wake();
        }
    }
}
impl Drop for RawWake {
    fn drop(&mut self) {
        self.requests.0.lock().unwrap().drops.push(self.key);
    }
}
pub(crate) struct DependencyWaker(Arc<RawWake>);

impl DependencyWaker {
    pub fn new(node: Weak<impl BindSink>, param: usize) -> Self {
        let node = node;
        Self(LazyTasks::with(|t| {
            t.wakes.insert(WeakTaskOf { node, param })
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
    pub fn oc_with<T>(&mut self, f: impl FnOnce(&mut ObsContext) -> T) -> T {
        let b = self.0.borrow_mut();
        let p: *mut ObsContext<'static> = *b;
        unsafe {
            let p: *mut ObsContext = transmute(p);
            f(&mut *p)
        }
    }
}
