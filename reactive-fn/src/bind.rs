use slabmap::SlabMap;
use std::cell::{RefCell, RefMut};
use std::mem::{drop, replace, swap};
use std::rc::{Rc, Weak};

pub struct BindContext<'a> {
    scope: &'a BindScope,
    bb: RefCell<BindingsBuilder>,
}

impl<'a> BindContext<'a> {
    pub fn bind(&self, source: Rc<impl BindSource>) {
        self.bb.borrow_mut().bind(source)
    }
    pub fn scope(&self) -> &BindScope {
        &self.scope
    }
}
struct BindingsBuilder {
    sink: Weak<dyn BindSink>,
    bindings: Vec<Binding>,
    len: usize,
}
impl BindingsBuilder {
    fn new(sink: Weak<dyn BindSink>, bindings: Vec<Binding>) -> Self {
        Self {
            sink,
            bindings,
            len: 0,
        }
    }
    pub fn bind(&mut self, source: Rc<dyn BindSource>) {
        if self.len < self.bindings.len() {
            if !Rc::ptr_eq(&self.bindings[self.len].source, &source) {
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
        let idx = source.attach_sink(sink);
        let binding = Binding { source, idx };
        self.bindings.push(binding);
    }

    fn build(mut self) -> Vec<Binding> {
        self.bindings.truncate(self.len);
        self.bindings
    }
}

pub trait BindSource: 'static {
    fn sinks(&self) -> &BindSinks;
    fn attach_sink(&self, sink: Weak<dyn BindSink>) -> usize {
        self.sinks().attach(sink)
    }
    fn detach_sink(&self, idx: usize) {
        self.sinks().detach(idx)
    }
}

pub trait BindSink: 'static {
    fn notify(self: Rc<Self>, scope: &NotifyScope);
}

struct Binding {
    source: Rc<dyn BindSource>,
    idx: usize,
}
impl Drop for Binding {
    fn drop(&mut self) {
        self.source.detach_sink(self.idx);
    }
}
pub struct Bindings(Vec<Binding>);

impl Bindings {
    pub fn new() -> Self {
        Self(Vec::new())
    }
    pub fn update<'a, T: 'a>(
        &mut self,
        scope: &'a BindScope,
        sink: &Rc<impl BindSink>,
        f: impl FnOnce(&BindContext<'a>) -> T,
    ) -> T {
        let bindings = replace(&mut self.0, Vec::new());
        let ctx = BindContext {
            scope,
            bb: RefCell::new(BindingsBuilder::new(
                Rc::downgrade(sink) as Weak<dyn BindSink>,
                bindings,
            )),
        };
        let value = f(&ctx);
        self.0 = ctx.bb.into_inner().build();
        value
    }

    pub fn clear(&mut self) {
        self.0.clear()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A collection of `BindSink`.
pub struct BindSinks {
    sinks: RefCell<SlabMap<Weak<dyn BindSink>>>,
    detach_idxs: RefCell<Vec<usize>>,
}

impl BindSinks {
    pub fn new() -> Self {
        Self {
            sinks: RefCell::new(SlabMap::new()),
            detach_idxs: RefCell::new(Vec::new()),
        }
    }
    pub fn notify(&self, scope: &NotifyScope) {
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
    pub fn is_empty(&self) -> bool {
        self.sinks.borrow().is_empty()
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

struct Runtime(BindScope);
pub struct BindScope(NotifyScope);
pub struct NotifyScope(RefCell<RuntimeData>);

struct RuntimeData {
    state: RuntimeState,
    defer_bind_tasks: Vec<Rc<dyn BindTask>>,
    defer_notify_sources: Vec<Rc<dyn BindSource>>,
}
enum RuntimeState {
    None,
    Notify,
    Bind,
}

pub trait BindTask: 'static {
    fn run(self: Rc<Self>, scope: &BindScope);
}
pub trait NotifyTask: 'static {
    fn run(self: Rc<Self>, scope: &NotifyScope);
}

impl NotifyScope {
    pub fn spawn(&self, task: Rc<dyn BindTask>) {
        self.0.borrow_mut().defer_bind_tasks.push(task);
    }
    pub fn with<T>(f: impl FnOnce(&NotifyScope) -> T) -> T {
        Runtime::with(|this| {
            this.notify(f, |_| {
                panic!("Cannot create NotifyContext when BindContext exists.")
            })
        })
    }
    pub fn update(s: &Rc<impl BindSource>) {
        Runtime::with(|this| {
            this.notify(
                |ctx| s.sinks().notify(ctx),
                |this| this.defer_notify_sources.push(s.clone()),
            )
        });
    }
}
impl BindScope {
    pub fn with<T>(f: impl FnOnce(&BindScope) -> T) -> T {
        Runtime::with(|this| this.bind(f))
    }
}

impl Runtime {
    fn new() -> Self {
        Self(BindScope(NotifyScope(RefCell::new(RuntimeData {
            state: RuntimeState::None,
            defer_bind_tasks: Vec::new(),
            defer_notify_sources: Vec::new(),
        }))))
    }
    fn notify<T>(
        &self,
        on_ctx: impl FnOnce(&NotifyScope) -> T,
        on_failed: impl FnOnce(&mut RuntimeData) -> T,
    ) -> T {
        let value;
        let mut b = self.borrow_mut();
        match b.state {
            RuntimeState::None => {
                b.state = RuntimeState::Notify;
                drop(b);
                value = on_ctx(self.notify_scope());
                self.notify_end(self.borrow_mut());
            }
            RuntimeState::Notify => {
                drop(b);
                value = on_ctx(self.notify_scope());
            }
            RuntimeState::Bind => {
                value = on_failed(&mut b);
            }
        }
        value
    }
    fn notify_end(&self, b: RefMut<RuntimeData>) {
        let mut b = b;
        if b.defer_bind_tasks.is_empty() {
            b.state = RuntimeState::None;
            return;
        }
        b.state = RuntimeState::Bind;
        while let Some(task) = b.defer_bind_tasks.pop() {
            drop(b);
            task.run(self.bind_scope());
            b = self.borrow_mut();
        }
        self.bind_end(b);
    }
    fn bind<T>(&self, f: impl FnOnce(&BindScope) -> T) -> T {
        let mut b = self.borrow_mut();
        let value;
        match b.state {
            RuntimeState::None => {
                b.state = RuntimeState::Bind;
                drop(b);
                value = f(self.bind_scope());
                self.bind_end(self.borrow_mut());
            }
            RuntimeState::Bind => {
                drop(b);
                value = f(self.bind_scope());
            }
            RuntimeState::Notify => {
                panic!("Cannot create BindContext when NotifyContext exists.");
            }
        }
        value
    }

    fn bind_end(&self, b: RefMut<RuntimeData>) {
        let mut b = b;
        b.state = RuntimeState::None;
        if b.defer_notify_sources.is_empty() {
            return;
        }
        b.state = RuntimeState::Notify;
        let mut sources = Vec::new();
        swap(&mut b.defer_notify_sources, &mut sources);
        drop(b);
        for s in &sources {
            s.sinks().notify(self.notify_scope());
        }
        sources.clear();
        b = self.borrow_mut();
        b.defer_notify_sources = sources;
        self.notify_end(b);
    }

    fn with<T>(f: impl FnOnce(&Self) -> T) -> T {
        thread_local!(static RT: Runtime = Runtime::new());
        RT.with(|data| f(data))
    }
    fn borrow_mut(&self) -> RefMut<RuntimeData> {
        ((self.0).0).0.borrow_mut()
    }
    fn notify_scope(&self) -> &NotifyScope {
        &(self.0).0
    }
    fn bind_scope(&self) -> &BindScope {
        &self.0
    }
}
