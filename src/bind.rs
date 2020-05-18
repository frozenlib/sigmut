use std::cell::RefCell;
use std::cmp::min;
use std::mem::{drop, swap};
use std::rc::{Rc, Weak};

pub struct BindContext {
    sink: Weak<dyn BindSink>,
    bindings: Vec<Binding>,
}
impl BindContext {
    pub fn bind(&mut self, source: Rc<impl BindSource>) {
        let sink = self.sink.clone();
        let idx = source.attach_sink(sink.clone());
        let binding = Binding { source, sink, idx };
        self.bindings.push(binding);
    }
}

pub trait BindSource: 'static {
    fn sinks(&self) -> &BindSinks;
    fn attach_sink(&self, sink: Weak<dyn BindSink>) -> usize {
        self.sinks().attach(sink)
    }
    fn detach_sink(&self, idx: usize, sink: &Weak<dyn BindSink>) {
        self.sinks().detach(idx, sink)
    }
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
        self.source.detach_sink(self.idx, &self.sink);
    }
}
pub struct Bindings(Vec<Binding>);

impl Bindings {
    pub fn new() -> Self {
        Self(Vec::new())
    }
    pub fn update<T>(
        &mut self,
        sink: &Rc<impl BindSink>,
        f: impl FnOnce(&mut BindContext) -> T,
    ) -> T {
        let mut ctx = BindContext {
            sink: Rc::downgrade(sink) as Weak<dyn BindSink>,
            bindings: Vec::new(),
        };
        let value = f(&mut ctx);
        self.0 = ctx.bindings;
        value
    }
    pub fn clear(&mut self) {
        self.0.clear()
    }
}

/// A collection of `BindSink`.
pub struct BindSinks(RefCell<BindSinkData>);
impl BindSinks {
    pub fn new() -> Self {
        Self(RefCell::new(BindSinkData::new()))
    }
    pub fn notify(&self, ctx: &NotifyContext) {
        let mut b = self.0.borrow();
        for idx in 0.. {
            if let Some(sink) = b.sinks.get(idx) {
                if let Some(sink) = Weak::upgrade(sink) {
                    drop(b);
                    sink.notify(ctx);
                    b = self.0.borrow();
                }
            } else {
                break;
            }
        }
    }
    fn extend_to(&self, sinks: &mut Vec<Weak<dyn BindSink>>) {
        for sink in &self.0.borrow().sinks {
            sinks.push(sink.clone());
        }
    }
    pub fn notify_root(&self) {
        NotifyContext::with(|ctx| ctx.notify_root(self));
    }
    pub fn is_empty(&self) -> bool {
        self.0.borrow_mut().is_empty()
    }
    pub fn attach(&self, sink: Weak<dyn BindSink>) -> usize {
        self.0.borrow_mut().attach(sink)
    }
    pub fn detach(&self, idx: usize, sink: &Weak<dyn BindSink>) {
        self.0.borrow_mut().detach(idx, sink);
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
    fn attach(&mut self, sink: Weak<dyn BindSink>) -> usize {
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
    fn detach(&mut self, idx: usize, sink: &Weak<dyn BindSink>) {
        if let Some(s) = self.sinks.get_mut(idx) {
            if Weak::ptr_eq(s, sink) {
                *s = freed_sink();
                self.idx_next = min(self.idx_next, idx);
            }
        }
    }

    fn is_empty(&mut self) -> bool {
        let remove_len = self
            .sinks
            .iter()
            .rev()
            .take_while(|sink| sink.strong_count() == 0)
            .count();
        let new_len = self.sinks.len() - remove_len;
        self.sinks.resize_with(new_len, || unreachable!());
        self.sinks.is_empty()
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
    state: NotifyContextState,
    tasks: Vec<Weak<dyn Task>>,
    lazy_notify_sinks: Vec<Weak<dyn BindSink>>,
    lazy_drop_bindings: Vec<Binding>,
}
enum NotifyContextState {
    None,
    Notify(usize),
    Bind(usize),
}

pub trait Task: 'static {
    fn run(self: Rc<Self>);
}

impl NotifyContext {
    fn new() -> Self {
        Self(RefCell::new(NotifyContextData {
            state: NotifyContextState::None,
            tasks: Vec::new(),
            lazy_notify_sinks: Vec::new(),
            lazy_drop_bindings: Vec::new(),
        }))
    }

    fn notify_root(&self, sinks: &BindSinks) {
        let mut b = self.0.borrow_mut();
        match b.state {
            NotifyContextState::None => {
                b.state = NotifyContextState::Notify(0);
                drop(b);
                sinks.notify(&self);
                self.end_notify(None);
            }
            NotifyContextState::Notify(depth) => {
                assert_ne!(depth, usize::MAX);
                b.state = NotifyContextState::Notify(depth + 1);
                drop(b);
                sinks.notify(self);
                b = self.0.borrow_mut();
                b.state = NotifyContextState::Notify(depth);
            }
            NotifyContextState::Bind(_) => {
                sinks.extend_to(&mut b.lazy_notify_sinks);
            }
        }
    }
    fn bind_root(&self, f: impl FnOnce(&NotifyContext)) {
        let mut b = self.0.borrow_mut();
        match b.state {
            NotifyContextState::None => {
                b.state = NotifyContextState::Bind(0);
                drop(b);
                f(self);
                self.end_bind();
            }
            NotifyContextState::Bind(depth) => {
                assert_ne!(depth, usize::MAX);
                b.state = NotifyContextState::Bind(depth + 1);
                drop(b);
                f(self);
                b = self.0.borrow_mut();
                b.state = NotifyContextState::Bind(depth);
            }
            NotifyContextState::Notify(_) => {
                panic!("Cannot call `Bindings::update` in `NotifySinks::notify`.");
            }
        }
    }
    fn lazy_drop_bindings(&self, bindings: impl IntoIterator<Item = Binding>) {
        let mut b = self.0.borrow_mut();
        assert!(matches!(b.state, NotifyContextState::Bind(_)));
        b.lazy_drop_bindings.extend(bindings);
    }

    fn end_notify(&self, lazy_notify_sinks: Option<Vec<Weak<dyn BindSink>>>) {
        let mut b = self.0.borrow_mut();
        if let Some(s) = lazy_notify_sinks {
            b.lazy_notify_sinks = s;
        }
        while let Some(task) = b.tasks.pop() {
            if let Some(task) = Weak::upgrade(&task) {
                drop(b);
                task.run();
                b = self.0.borrow_mut();
            }
        }
        b.state = NotifyContextState::None;
    }
    fn end_bind(&self) {
        let mut b = self.0.borrow_mut();
        b.state = NotifyContextState::None;
        b.lazy_drop_bindings.clear();
        if !b.lazy_notify_sinks.is_empty() {
            b.state = NotifyContextState::Notify(0);
            let mut sinks = Vec::new();
            swap(&mut b.lazy_notify_sinks, &mut sinks);
            drop(b);
            for sink in &sinks {
                if let Some(sink) = Weak::upgrade(sink) {
                    sink.notify(self);
                }
            }
            sinks.clear();
            b = self.0.borrow_mut();
            self.end_notify(Some(sinks));
        }
    }

    fn with<U>(f: impl Fn(&Self) -> U) -> U {
        thread_local!(static CTX: NotifyContext = NotifyContext::new());
        CTX.with(|data| f(data))
    }

    pub fn spawn(&self, task: Weak<impl Task>) {
        self.0.borrow_mut().tasks.push(task);
    }
}
