use slab::Slab;
use std::cell::{RefCell, RefMut};
use std::mem::{drop, replace, swap};
use std::rc::{Rc, Weak};

pub struct BindContext<'a> {
    scope: &'a BindContextScope,
    bb: RefCell<BindingsBuilder>,
}

impl<'a> BindContext<'a> {
    pub fn bind(&self, source: Rc<impl BindSource>) {
        self.bb.borrow_mut().bind(source)
    }
    pub fn scope(&self) -> &BindContextScope {
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
        if self.bindings.len() == self.len {
            self.bind_new(source)
        } else {
            if !Rc::ptr_eq(&self.bindings[self.len].source, &source) {
                let idx_old = self.len;
                let idx_new = self.bindings.len();
                self.bind_new(source);
                self.bindings.swap(idx_old, idx_new);
            }
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
        self.bindings.drain(self.len..);
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
    fn notify(self: Rc<Self>, ctx: &NotifyContext);
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
    pub fn update<T>(
        &mut self,
        scope: &BindContextScope,
        sink: &Rc<impl BindSink>,
        f: impl FnOnce(&BindContext) -> T,
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
}

/// A collection of `BindSink`.
pub struct BindSinks {
    sinks: RefCell<Slab<Weak<dyn BindSink>>>,
    detach_idxs: RefCell<Vec<usize>>,
}

impl BindSinks {
    pub fn new() -> Self {
        Self {
            sinks: RefCell::new(Slab::new()),
            detach_idxs: RefCell::new(Vec::new()),
        }
    }
    pub fn notify(&self, ctx: &NotifyContext) {
        let mut sinks = self.sinks.borrow_mut();
        for (_, sink) in sinks.iter() {
            if let Some(sink) = Weak::upgrade(sink) {
                sink.notify(ctx);
            }
        }
        let mut detach_idxs = self.detach_idxs.borrow_mut();
        for &idx in detach_idxs.iter() {
            sinks.remove(idx);
        }
        detach_idxs.clear();
    }
    fn extend_to(&self, sinks: &mut Vec<Weak<dyn BindSink>>) {
        for (_, sink) in &*self.sinks.borrow() {
            sinks.push(sink.clone());
        }
    }
    pub fn notify_and_update(&self) {
        NotifyContext::notify_and_update(self);
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

struct ReactiveContext(BindContextScope);
pub struct BindContextScope(NotifyContext);
pub struct NotifyContext(RefCell<ReactiveContextData>);

struct ReactiveContextData {
    state: ReactiveState,
    lazy_tasks: Vec<Weak<dyn Task>>,
    lazy_notify_sinks: Vec<Weak<dyn BindSink>>,
}
enum ReactiveState {
    None,
    Notify(usize),
    Bind(usize),
}

pub trait Task: 'static {
    fn run(self: Rc<Self>, scope: &BindContextScope);
}

impl NotifyContext {
    pub fn spawn(&self, task: Weak<impl Task>) {
        self.0.borrow_mut().lazy_tasks.push(task);
    }
    pub fn with<T>(f: impl FnOnce(&NotifyContext) -> T) -> T {
        ReactiveContext::with(|this| {
            this.notify(f, |_| {
                panic!("Cannot create NotifyContext when BindContext exists.")
            })
        })
    }
    pub fn notify_and_update(sinks: &BindSinks) {
        ReactiveContext::with(|this| {
            this.notify(
                |ctx| sinks.notify(ctx),
                |this| sinks.extend_to(&mut this.lazy_notify_sinks),
            )
        });
    }
}
impl BindContextScope {
    pub fn with<T>(f: impl FnOnce(&BindContextScope) -> T) -> T {
        ReactiveContext::with(|this| this.bind(f))
    }
}

impl ReactiveContext {
    fn new() -> Self {
        Self(BindContextScope(NotifyContext(RefCell::new(
            ReactiveContextData {
                state: ReactiveState::None,
                lazy_tasks: Vec::new(),
                lazy_notify_sinks: Vec::new(),
            },
        ))))
    }
    fn notify<T>(
        &self,
        on_ctx: impl FnOnce(&NotifyContext) -> T,
        on_failed: impl FnOnce(&mut ReactiveContextData) -> T,
    ) -> T {
        let value;
        let mut b = self.borrow_mut();
        match b.state {
            ReactiveState::None => {
                b.state = ReactiveState::Notify(0);
                drop(b);
                value = on_ctx(self.notify_ctx());
                self.notify_end(self.borrow_mut());
            }
            ReactiveState::Notify(depth) => {
                assert_ne!(depth, usize::MAX);
                b.state = ReactiveState::Notify(depth + 1);
                drop(b);
                value = on_ctx(self.notify_ctx());
                self.borrow_mut().state = ReactiveState::Notify(depth);
            }
            ReactiveState::Bind(_) => {
                value = on_failed(&mut b);
            }
        }
        value
    }
    fn notify_end(&self, b: RefMut<ReactiveContextData>) {
        let mut b = b;
        if b.lazy_tasks.is_empty() {
            b.state = ReactiveState::None;
            return;
        }
        b.state = ReactiveState::Bind(0);
        while let Some(task) = b.lazy_tasks.pop() {
            if let Some(task) = Weak::upgrade(&task) {
                drop(b);
                task.run(self.bind_ctx_scope());
                b = self.borrow_mut();
            }
        }
        self.bind_end(b);
    }
    fn bind<T>(&self, f: impl FnOnce(&BindContextScope) -> T) -> T {
        let mut b = self.borrow_mut();
        let value;
        match b.state {
            ReactiveState::None => {
                b.state = ReactiveState::Bind(0);
                drop(b);
                value = f(self.bind_ctx_scope());
                self.bind_end(self.borrow_mut());
            }
            ReactiveState::Bind(depth) => {
                assert_ne!(depth, usize::MAX);
                b.state = ReactiveState::Bind(depth + 1);
                drop(b);
                value = f(self.bind_ctx_scope());
                self.borrow_mut().state = ReactiveState::Bind(depth);
            }
            ReactiveState::Notify(_) => {
                panic!("Cannot create BindContext when NotifyContext exists.");
            }
        }
        value
    }

    fn bind_end(&self, b: RefMut<ReactiveContextData>) {
        let mut b = b;
        b.state = ReactiveState::None;
        if b.lazy_notify_sinks.is_empty() {
            return;
        }
        b.state = ReactiveState::Notify(0);
        let mut sinks = Vec::new();
        swap(&mut b.lazy_notify_sinks, &mut sinks);
        drop(b);
        for sink in &sinks {
            if let Some(sink) = Weak::upgrade(sink) {
                sink.notify(self.notify_ctx());
            }
        }
        sinks.clear();
        b = self.borrow_mut();
        b.lazy_notify_sinks = sinks;
        self.notify_end(b);
    }

    fn with<T>(f: impl FnOnce(&Self) -> T) -> T {
        thread_local!(static CTX: ReactiveContext = ReactiveContext::new());
        CTX.with(|data| f(data))
    }
    fn borrow_mut(&self) -> RefMut<ReactiveContextData> {
        ((self.0).0).0.borrow_mut()
    }
    fn notify_ctx(&self) -> &NotifyContext {
        &(self.0).0
    }
    fn bind_ctx_scope(&self) -> &BindContextScope {
        &self.0
    }
}
