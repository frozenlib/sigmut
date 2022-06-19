use rt_local_core::spawn_local;
use slabmap::SlabMap;
use std::{cell::Cell, mem};
use std::{
    cell::RefCell,
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
    task::{Context, Poll, Waker},
};

pub struct BindScope {
    _dummy: (),
}

impl BindScope {
    pub fn with<T>(f: impl FnOnce(&BindScope) -> T) -> T {
        Runtime::with(|rt| rt.bind_start());
        let value = f(&BindScope { _dummy: () });
        if !Runtime::with(|rt| rt.bind_end()) {
            run_all_notify_tasks();
        }
        value
    }
}

pub struct NotifyScope {
    _dummy: (),
}

impl NotifyScope {
    pub fn with<T>(f: impl FnOnce(&NotifyScope) -> T) -> T {
        Runtime::with(|rt| rt.notify_start());
        let value = f(&NotifyScope { _dummy: () });
        Runtime::with(|rt| rt.notify_end());
        value
    }
}

pub struct BindContext<'a> {
    scope: &'a BindScope,
    bb: Option<BindingsBuilder>,
}

impl<'a> BindContext<'a> {
    pub fn bind(&mut self, source: Rc<impl BindSource>) {
        if let Some(bb) = &mut self.bb {
            bb.bind(source);
        }
    }
    pub fn scope(&self) -> &BindScope {
        self.scope
    }
    pub fn null<T>(f: impl FnOnce(&mut BindContext) -> T) -> T {
        BindScope::with(|scope| f(&mut BindContext { scope, bb: None }))
    }
}
struct BindingsBuilder {
    sink: Weak<dyn BindSink>,
    sink_changed: bool,
    bindings: Vec<Binding>,
    len: usize,
}
impl BindingsBuilder {
    fn new(sink: Weak<dyn BindSink>, sink_changed: bool, bindings: Vec<Binding>) -> Self {
        Self {
            sink,
            sink_changed,
            bindings,
            len: 0,
        }
    }
    pub fn bind(&mut self, source: Rc<dyn BindSource>) {
        if self.len < self.bindings.len() {
            #[allow(clippy::vtable_address_comparisons)]
            // The purpose of this `if` is little optimization,
            // so it doesn't matter if the block is executed by different vtable address.
            if self.sink_changed || !Rc::ptr_eq(&self.bindings[self.len].source, &source) {
                let idx_old = self.len;
                let idx_new = self.bindings.len();
                self.bind_new(source);
                self.bindings.swap(idx_old, idx_new);
            }
        } else {
            self.bind_new(source)
        }
        self.len += 1;
    }
    fn bind_new(&mut self, source: Rc<dyn BindSource>) {
        let sink = self.sink.clone();
        let idx = source.sinks().attach(sink);
        let binding = Binding { source, idx };
        self.bindings.push(binding);
    }

    fn build(mut self) -> Vec<Binding> {
        self.bindings.truncate(self.len);
        self.bindings
    }
}

pub trait BindSink: 'static {
    fn notify(self: Rc<Self>, scope: &NotifyScope);
}

pub trait BindSource: 'static {
    fn sinks(&self) -> &BindSinks;
    fn on_sinks_empty(self: Rc<Self>) {}
}
pub fn schedule_notify(source: &Rc<impl BindSource>) {
    if Runtime::with(|rt| {
        if rt.depth_bind == 0 {
            rt.notify_start();
            true
        } else {
            if source.sinks().set_scheduled() {
                rt.push_notify(source.clone());
            }
            false
        }
    }) {
        source.sinks().notify(&NotifyScope { _dummy: () });
        Runtime::with(|rt| rt.notify_end());
    }
}
fn run_all_notify_tasks() {
    NotifyScope::with(|scope| {
        while let Some(s) = Runtime::with(|rt| rt.notify_sources.pop_front()) {
            s.sinks().notify(scope);
        }
    });
}

pub trait BindTask: 'static {
    fn run(self: Rc<Self>, scope: &BindScope);
}
pub fn schedule_bind(task: &Rc<impl BindTask>) {
    schedule_bind_raw(task.clone())
}
pub fn schedule_bind_raw(task: Rc<dyn BindTask>) {
    Runtime::with(|rt| rt.push_bind(task));
}

struct Binding {
    source: Rc<dyn BindSource>,
    idx: usize,
}
impl Drop for Binding {
    fn drop(&mut self) {
        let sinks = self.source.sinks();
        sinks.detach(self.idx);
        if sinks.is_empty() {
            self.source.clone().on_sinks_empty();
        }
    }
}
pub struct Bindings {
    bindings: Vec<Binding>,
    sink: Weak<dyn BindSink>,
}
impl Bindings {
    pub fn new() -> Self {
        struct DummyBindSink;
        impl BindSink for DummyBindSink {
            fn notify(self: Rc<Self>, _scope: &NotifyScope) {}
        }
        Self {
            bindings: Vec::new(),
            sink: Weak::new() as Weak<DummyBindSink>,
        }
    }
    pub fn update<T>(
        &mut self,
        scope: &BindScope,
        sink: &Rc<impl BindSink>,
        f: impl FnOnce(&mut BindContext) -> T,
    ) -> T {
        let bindings = mem::take(&mut self.bindings);
        let sink = Rc::downgrade(sink) as Weak<dyn BindSink>;
        let sink_changed = !Weak::ptr_eq(&self.sink, &sink);
        if sink_changed {
            self.sink = sink.clone();
        }
        let mut bc = BindContext {
            scope,
            bb: Some(BindingsBuilder::new(sink, sink_changed, bindings)),
        };
        let value = f(&mut bc);
        self.bindings = bc.bb.unwrap().build();
        value
    }

    pub fn clear(&mut self) {
        self.bindings.clear()
    }
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}
impl Default for Bindings {
    fn default() -> Self {
        Self::new()
    }
}

/// A collection of `BindSink`.
#[derive(Default)]
pub struct BindSinks {
    sinks: RefCell<SlabMap<Weak<dyn BindSink>>>,
    detach_idxs: RefCell<Vec<usize>>,
    scheduled: Cell<bool>,
}

impl BindSinks {
    pub fn new() -> Self {
        Default::default()
    }
    pub fn notify(&self, scope: &NotifyScope) {
        self.scheduled.set(false);
        let mut sinks = self.sinks.borrow_mut();
        sinks.optimize();
        for (_, sink) in sinks.iter() {
            if let Some(sink) = Weak::upgrade(sink) {
                sink.notify(scope);
            }
        }
        let mut detach_idxs = self.detach_idxs.borrow_mut();
        for &idx in detach_idxs.iter() {
            sinks.remove(idx);
        }
        detach_idxs.clear();
    }
    pub fn notify_with_new_scope(&self) {
        NotifyScope::with(|scope| self.notify(scope))
    }
    pub fn is_empty(&self) -> bool {
        self.sinks.borrow().is_empty()
    }
    fn set_scheduled(&self) -> bool {
        if !self.is_empty() && !self.scheduled.get() {
            self.scheduled.set(true);
            true
        } else {
            false
        }
    }
    pub fn attach(&self, sink: Weak<dyn BindSink>) -> usize {
        self.sinks.borrow_mut().insert(sink)
    }
    pub fn detach(&self, idx: usize) {
        if let Ok(mut b) = self.sinks.try_borrow_mut() {
            b.remove(idx);
        } else {
            self.detach_idxs.borrow_mut().push(idx);
        }
    }
}

thread_local! {
    static RUNTIME: RefCell<Option<Runtime>> = RefCell::new(None);
}

struct Runtime {
    depth_notify: usize,
    depth_bind: usize,
    notify_sources: VecDeque<Rc<dyn BindSource>>,
    bind_tasks: VecDeque<Rc<dyn BindTask>>,
    waker: Option<Waker>,
}
impl Runtime {
    fn new() -> Self {
        spawn_local(TaskRunner).detach();
        Self {
            depth_notify: 0,
            depth_bind: 0,
            notify_sources: VecDeque::new(),
            bind_tasks: VecDeque::new(),
            waker: None,
        }
    }
    fn with<T>(f: impl FnOnce(&mut Runtime) -> T) -> T {
        RUNTIME.with(|rt| f(rt.borrow_mut().get_or_insert_with(Self::new)))
    }
    fn push_bind(&mut self, task: Rc<dyn BindTask>) {
        self.bind_tasks.push_back(task);
        self.wake();
    }
    fn push_notify(&mut self, source: Rc<dyn BindSource>) {
        self.notify_sources.push_back(source);
    }
    fn wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
    fn bind_start(&mut self) {
        if self.depth_notify != 0 {
            panic!("cannot start bind in notify.");
        }
        if self.depth_bind == usize::MAX {
            panic!("bind count overflow.");
        }
        self.depth_bind += 1;
    }
    fn bind_end(&mut self) -> bool {
        self.depth_bind -= 1;
        self.depth_bind != 0
    }
    fn notify_start(&mut self) {
        if self.depth_bind != 0 {
            panic!("cannot start notify in bind.");
        }
        if self.depth_notify == usize::MAX {
            panic!("notify count overflow.");
        }
        self.depth_notify += 1;
    }
    fn notify_end(&mut self) {
        self.depth_notify -= 1;
    }
}
struct TaskRunner;

impl Future for TaskRunner {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        loop {
            run_all_notify_tasks();
            if let Some(task) = Runtime::with(|rt| rt.bind_tasks.pop_front()) {
                BindScope::with(|scope| task.run(scope));
            } else {
                Runtime::with(|rt| rt.waker = Some(cx.waker().clone()));
                return Poll::Pending;
            }
        }
    }
}
