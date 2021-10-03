use crate::{BindScope, NotifyScope, NotifyTask};
use slabmap::SlabMap;
use std::cell::RefCell;
use std::mem;
use std::rc::{Rc, Weak};

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
    pub fn nul<T>(f: impl FnOnce(&mut BindContext) -> T) -> T {
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
}

impl BindSinks {
    pub fn new() -> Self {
        Default::default()
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
