use std::cell::RefCell;
use std::cmp::min;
use std::mem::drop;
use std::mem::replace;
use std::rc::{Rc, Weak};

pub struct ReContext<'a> {
    sink: Weak<dyn BindSink>,
    bindings: &'a mut Vec<Binding>,
}
impl<'a> ReContext<'a> {
    pub fn new(sink: &Rc<impl BindSink + 'static>, bindings: &'a mut Vec<Binding>) -> Self {
        debug_assert!(bindings.is_empty());
        Self {
            sink: Rc::downgrade(sink) as Weak<dyn BindSink>,
            bindings,
        }
    }
    pub fn bind(&mut self, source: Rc<impl BindSource>) {
        let sink = self.sink.clone();
        let idx = source.sinks().insert(sink.clone());
        let binding = Binding { source, sink, idx };
        self.bindings.push(binding);
    }
}

pub trait BindSource: 'static {
    fn sinks(&self) -> &BindSinks;
}

pub trait BindSink: 'static {
    fn notify(self: Rc<Self>, ctx: &NotifyContext);
}

pub struct Binding {
    source: Rc<dyn BindSource>,
    sink: Weak<dyn BindSink>,
    idx: usize,
}
impl Drop for Binding {
    fn drop(&mut self) {
        self.source.sinks().remove(self.idx, &self.sink);
    }
}

/// A collection of `BindSink`.
pub struct BindSinks(RefCell<Option<BindSinkData>>);
impl BindSinks {
    pub fn new() -> Self {
        Self(RefCell::new(Some(BindSinkData::new())))
    }
    pub fn notify_with(&self, ctx: &NotifyContext) {
        let mut sinks = self
            .0
            .borrow_mut()
            .take()
            .expect("`BindSinks::notify` called during notify process.");
        sinks.notify(ctx);
        *self.0.borrow_mut() = Some(sinks);
    }
    pub fn notify(&self) {
        NotifyContext::with(|ctx| self.notify_with(ctx));
    }
    pub fn is_empty(&self) -> bool {
        let b = self.0.borrow();
        if let Some(sinks) = &*b {
            sinks.is_empty()
        } else {
            true
        }
    }
    fn insert(&self, sink: Weak<dyn BindSink>) -> usize {
        self.0
            .borrow_mut()
            .as_mut()
            .expect("`BindSinks::insert` called during notify process.")
            .insert(sink)
    }
    fn remove(&self, idx: usize, sink: &Weak<dyn BindSink>) {
        if let Some(sinks) = &mut *self.0.borrow_mut() {
            sinks.remove(idx, sink);
        }
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
    fn insert(&mut self, sink: Weak<dyn BindSink>) -> usize {
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
    fn is_empty(&self) -> bool {
        self.sinks.iter().all(|x| x.strong_count() == 0)
    }
}
fn freed_sink() -> Weak<dyn BindSink> {
    struct DummyBindSink;
    impl BindSink for DummyBindSink {
        fn notify(self: Rc<Self>, _ctx: &NotifyContext) {}
    }
    Weak::<DummyBindSink>::new()
}

/// The context of `BindSink::notify`.
pub struct NotifyContext(RefCell<NotifyContextData>);
struct NotifyContextData {
    ref_count: usize,
    tasks: Vec<Weak<dyn Task>>,
}

pub trait Task: 'static {
    fn run(self: Rc<Self>);
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
                if let Some(task) = Weak::upgrade(&task) {
                    drop(d);
                    task.run();
                    d = self.0.borrow_mut();
                }
            }
        }
        d.ref_count -= 1;
    }

    pub fn spawn(&self, task: Weak<impl Task>) {
        self.0.borrow_mut().tasks.push(task);
    }
}
