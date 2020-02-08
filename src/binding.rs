use std::cell::RefCell;
use std::cmp::min;
use std::mem::{drop, replace};
use std::rc::{Rc, Weak};

pub trait BindSource: 'static {
    fn sinks(&self) -> &BindSinks;
    fn bind(self: Rc<Self>, sink: &Rc<dyn BindSink>) -> Binding
    where
        Self: Sized,
    {
        let idx = self.sinks().0.borrow_mut().insert(sink);
        Binding {
            source: self,
            sink: Rc::downgrade(sink),
            idx,
        }
    }
}

pub trait BindSink {
    fn notify(&self, ctx: &NotifyContext);
}
pub struct NotifyContext(RefCell<NotifyContextData>);
struct NotifyContextData {
    ref_count: usize,
    tasks: Vec<Rc<dyn Task>>,
}

pub struct Binding {
    source: Rc<dyn BindSource>,
    sink: Weak<dyn BindSink>,
    idx: usize,
}
impl Drop for Binding {
    fn drop(&mut self) {
        self.source
            .sinks()
            .0
            .borrow_mut()
            .remove(self.idx, &self.sink);
    }
}

pub trait Task {
    fn run(&self);
}

impl NotifyContext {
    fn new() -> Self {
        Self(RefCell::new(NotifyContextData {
            ref_count: 0,
            tasks: Vec::new(),
        }))
    }

    pub fn with(f: impl Fn(&NotifyContext)) {
        thread_local!(static CTX: NotifyContext = NotifyContext::new());
        CTX.with(|ctx| {
            ctx.enter();
            f(ctx);
            ctx.leave();
        });
    }
    fn enter(&self) {
        let mut d = self.0.borrow_mut();
        assert!(d.ref_count != usize::max_value());
        d.ref_count += 1;
    }
    fn leave(&self) {
        let mut d = self.0.borrow_mut();
        assert!(d.ref_count != 0);
        if d.ref_count == 1 {
            while let Some(task) = d.tasks.pop() {
                drop(d);
                task.run();
                d = self.0.borrow_mut();
            }
        }
        d.ref_count -= 1;
    }

    pub fn push_task(&mut self, task: Rc<dyn Task>) {
        self.0.borrow_mut().tasks.push(task);
    }
}

pub struct BindSinks(RefCell<BindSinkData>);
impl BindSinks {
    pub fn new() -> Self {
        Self(RefCell::new(BindSinkData::new()))
    }
    pub fn notify_with(&self, ctx: &NotifyContext) {
        self.0.borrow_mut().notify(ctx);
    }
    pub fn notify(&self) {
        NotifyContext::with(|ctx| self.notify_with(ctx));
    }
}

struct BindSinkData {
    sinks: Vec<Weak<dyn BindSink>>,
    idx_next: usize,
}
impl BindSinkData {
    fn new() -> Self {
        Self {
            sinks: Vec::new(),
            idx_next: 0,
        }
    }
    fn insert(&mut self, sink: &Rc<dyn BindSink>) -> usize {
        let sink = Rc::downgrade(sink);
        while self.idx_next < self.sinks.len() {
            if self.sinks[self.idx_next].strong_count() == 0 {
                let idx = self.idx_next;
                self.sinks[idx] = sink;
                self.idx_next += 1;
                return idx;
            }
        }
        let idx = self.sinks.len();
        self.sinks.push(sink);
        idx
    }
    fn remove(&mut self, idx: usize, sink: &Weak<dyn BindSink>) {
        if let Some(s) = self.sinks.get_mut(idx) {
            if Weak::ptr_eq(s, sink) {
                *s = freed_sink();
                self.idx_next = min(self.idx_next, idx);
            }
        }
    }

    fn notify(&mut self, ctx: &NotifyContext) {
        for s in &mut self.sinks {
            if let Some(sink) = Weak::upgrade(&replace(s, freed_sink())) {
                sink.notify(ctx);
            }
        }
        self.sinks.clear();
        self.idx_next = 0;
    }
}
fn freed_sink() -> Weak<dyn BindSink> {
    struct DummyBindSink;
    impl BindSink for DummyBindSink {
        fn notify(&self, _ctx: &NotifyContext) {}
    }
    Weak::<DummyBindSink>::new()
}
pub struct BindContext {
    sink: Rc<dyn BindSink>,
    bindings: Vec<Binding>,
}
impl BindContext {
    pub fn new(sink: Rc<dyn BindSink>, bindings: Vec<Binding>) -> Self {
        Self { sink, bindings }
    }
    pub fn bind(&mut self, src: Rc<impl BindSource>) {
        self.bindings.push(src.bind(&self.sink));
    }
    pub fn into_bindings(self) -> Vec<Binding> {
        self.bindings
    }
}
