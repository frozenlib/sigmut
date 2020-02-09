use std::cell::RefCell;
use std::cmp::min;
use std::mem::replace;
use std::rc::{Rc, Weak};

use crate::NotifyContext;

pub trait BindSource: 'static {
    fn sinks(&self) -> &BindSinks;
    fn bind(self: Rc<Self>, sink: Weak<dyn BindSink>) -> Binding
    where
        Self: Sized,
    {
        let idx = self.sinks().0.borrow_mut().insert(sink.clone());
        Binding {
            source: self,
            sink,
            idx,
        }
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
        self.source
            .sinks()
            .0
            .borrow_mut()
            .remove(self.idx, &self.sink);
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
}
fn freed_sink() -> Weak<dyn BindSink> {
    struct DummyBindSink;
    impl BindSink for DummyBindSink {
        fn notify(self: Rc<Self>, _ctx: &NotifyContext) {}
    }
    Weak::<DummyBindSink>::new()
}
