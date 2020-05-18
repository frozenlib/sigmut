use std::cell::{RefCell, RefMut};
use std::cmp::min;
use std::mem::{drop, replace, swap};
use std::rc::{Rc, Weak};

pub struct BindContext<'a> {
    scope: &'a BindContextScope,
    sink: Weak<dyn BindSink>,
    bindings: Vec<Binding>,
}
impl<'a> BindContext<'a> {
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
    pub fn update_root<T>(
        &mut self,
        sink: &Rc<impl BindSink>,
        f: impl FnOnce(&mut BindContext) -> T,
    ) -> T {
        ReactiveContext::update_root(|scope| self.update_with(scope, sink, f))
    }
    pub fn update<T>(
        &mut self,
        ctx: &BindContext,
        sink: &Rc<impl BindSink>,
        f: impl FnOnce(&mut BindContext) -> T,
    ) -> T {
        self.update_with(ctx.scope, sink, f)
    }

    fn update_with<T>(
        &mut self,
        scope: &BindContextScope,
        sink: &Rc<impl BindSink>,
        f: impl FnOnce(&mut BindContext) -> T,
    ) -> T {
        let mut ctx = BindContext {
            scope,
            sink: Rc::downgrade(sink) as Weak<dyn BindSink>,
            bindings: Vec::new(),
        };
        let value = f(&mut ctx);
        scope.lazy_drop_bindings(replace(&mut self.0, ctx.bindings));
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
        ReactiveContext::notify_root(self);
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

struct ReactiveContext(BindContextScope);
pub struct BindContextScope(NotifyContext);
pub struct NotifyContext(RefCell<ReactiveContextData>);

struct ReactiveContextData {
    state: ReactiveState,
    lazy_tasks: Vec<Weak<dyn Task>>,
    lazy_notify_sinks: Vec<Weak<dyn BindSink>>,
    lazy_drop_bindings: Vec<Binding>,
}
enum ReactiveState {
    None,
    Notify(usize),
    Bind(usize),
}

pub trait Task: 'static {
    fn run(self: Rc<Self>);
}

impl NotifyContext {
    pub fn spawn(&self, task: Weak<impl Task>) {
        self.0.borrow_mut().lazy_tasks.push(task);
    }
}
impl BindContextScope {
    fn lazy_drop_bindings(&self, bindings: impl IntoIterator<Item = Binding>) {
        let mut b = ((self.0).0).borrow_mut();
        assert!(matches!(b.state, ReactiveState::Bind(_)));
        b.lazy_drop_bindings.extend(bindings);
    }
}

impl ReactiveContext {
    fn new() -> Self {
        Self(BindContextScope(NotifyContext(RefCell::new(
            ReactiveContextData {
                state: ReactiveState::None,
                lazy_tasks: Vec::new(),
                lazy_notify_sinks: Vec::new(),
                lazy_drop_bindings: Vec::new(),
            },
        ))))
    }

    fn notify_root(sinks: &BindSinks) {
        Self::with(|this| {
            let mut b = this.borrow_mut();
            match b.state {
                ReactiveState::None => {
                    b.state = ReactiveState::Notify(0);
                    drop(b);
                    sinks.notify(this.notify_ctx());
                    this.end_notify(None);
                }
                ReactiveState::Notify(depth) => {
                    assert_ne!(depth, usize::MAX);
                    b.state = ReactiveState::Notify(depth + 1);
                    drop(b);
                    sinks.notify(this.notify_ctx());
                    b = this.borrow_mut();
                    b.state = ReactiveState::Notify(depth);
                }
                ReactiveState::Bind(_) => {
                    sinks.extend_to(&mut b.lazy_notify_sinks);
                }
            }
        });
    }
    fn update_root<T>(f: impl FnOnce(&BindContextScope) -> T) -> T {
        Self::with(move |this| {
            let mut b = this.borrow_mut();
            let value;
            match b.state {
                ReactiveState::None => {
                    b.state = ReactiveState::Bind(0);
                    drop(b);
                    value = f(this.bind_ctx_scope());
                    this.end_bind();
                }
                ReactiveState::Bind(depth) => {
                    assert_ne!(depth, usize::MAX);
                    b.state = ReactiveState::Bind(depth + 1);
                    drop(b);
                    value = f(this.bind_ctx_scope());
                    b = this.borrow_mut();
                    b.state = ReactiveState::Bind(depth);
                }
                ReactiveState::Notify(_) => {
                    panic!("Cannot call `Bindings::update` in `NotifySinks::notify`.");
                }
            }
            value
        })
    }

    fn end_notify(&self, lazy_notify_sinks: Option<Vec<Weak<dyn BindSink>>>) {
        let mut b = self.borrow_mut();
        if let Some(s) = lazy_notify_sinks {
            b.lazy_notify_sinks = s;
        }
        while let Some(task) = b.lazy_tasks.pop() {
            if let Some(task) = Weak::upgrade(&task) {
                drop(b);
                task.run();
                b = self.borrow_mut();
            }
        }
        b.state = ReactiveState::None;
    }
    fn end_bind(&self) {
        let mut b = self.borrow_mut();
        b.state = ReactiveState::None;
        b.lazy_drop_bindings.clear();
        if !b.lazy_notify_sinks.is_empty() {
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
            self.end_notify(Some(sinks));
        }
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
