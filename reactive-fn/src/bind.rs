use crate::{BindScope, NotifyScope, NotifyTask};
use slabmap::SlabMap;
use std::cell::RefCell;
use std::mem::replace;
use std::rc::{Rc, Weak};

pub struct BindContext<'a> {
    scope: &'a BindScope,
    bb: Option<RefCell<BindingsBuilder>>,
}

impl<'a> BindContext<'a> {
    pub fn bind(&self, source: Rc<impl BindSource>) {
        if let Some(bb) = &self.bb {
            bb.borrow_mut().bind(source);
        }
    }
    pub fn scope(&self) -> &BindScope {
        &self.scope
    }
    pub fn with_no_sink<T>(f: impl FnOnce(&BindContext) -> T) -> T {
        BindScope::with(|scope| f(&BindContext { scope, bb: None }))
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
impl<T: BindSource> NotifyTask for T {
    fn run(self: Rc<Self>, scope: &NotifyScope) {
        self.sinks().notify(scope)
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
        let cx = BindContext {
            scope,
            bb: Some(RefCell::new(BindingsBuilder::new(
                Rc::downgrade(sink) as Weak<dyn BindSink>,
                bindings,
            ))),
        };
        let value = f(&cx);
        self.0 = cx.bb.unwrap().into_inner().build();
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
