use std::cell::RefCell;
use std::mem::{drop, replace};
use std::rc::{Rc, Weak};

pub trait BindSource {
    fn sinks(&self) -> &BindSinks;

    fn bind(&self, sink: &Rc<dyn BindSink>) -> Binding {
        self.sinks().bind(sink)
    }
    fn unbind(&self, binding: Binding, sink: &Weak<dyn BindSink>) {
        self.sinks().unbind(binding, sink);
    }
}
pub trait BindSink {
    fn lock(&self);
    fn unlock(&self, modified: bool);
}
pub struct Binding {
    idx: usize,
}

pub struct BindSinks(RefCell<BindSinksData>);
struct BindSinksData {
    sinks: Vec<BindSinkEntry>,
    locked: usize,
    modified: bool,
}
struct BindSinkEntry {
    sink: Weak<dyn BindSink>,
    locked: bool,
    modified: bool,
}

impl BindSinks {
    pub fn new() -> Self {
        Self(RefCell::new(BindSinksData {
            sinks: Vec::new(),
            locked: 0,
            modified: false,
        }))
    }
    pub fn bind(&self, sink: &Rc<dyn BindSink>) -> Binding {
        let mut b = self.0.borrow_mut();
        let locked = b.locked != 0;
        let s = BindSinkEntry {
            sink: Rc::downgrade(sink),
            locked,
            modified: false,
        };
        let mut idx = 0;
        loop {
            if idx == b.sinks.len() {
                b.sinks.push(s);
                break;
            }
            if let None = Weak::upgrade(&b.sinks[idx].sink) {
                b.sinks[idx] = s;
                break;
            }
            idx += 1;
        }
        if locked {
            drop(b);
            sink.lock();
        }
        Binding { idx }
    }
    pub fn unbind(&self, binding: Binding, sink: &Weak<dyn BindSink>) {
        struct DummyBindSink;
        impl BindSink for DummyBindSink {
            fn lock(&self) {}
            fn unlock(&self, _modified: bool) {}
        }
        let Binding { idx } = binding;
        let mut b = self.0.borrow_mut();
        if let Some(s) = b.sinks.get_mut(idx) {
            if s.sink.ptr_eq(sink) {
                let locked = s.locked;
                s.sink = Weak::<DummyBindSink>::new();
                s.locked = false;
                if locked {
                    if let Some(sink) = sink.upgrade() {
                        sink.unlock(false);
                    }
                }
            }
        }
    }
    pub fn lock(&self) {
        let mut b = self.0.borrow_mut();
        if b.locked == usize::max_value() {
            panic!("BindSource : too many locked.")
        }
        b.locked += 1;
        if b.locked > 1 {
            return;
        }

        let mut idx = 0;
        while let Some(sink) = b.sinks.get_mut(idx) {
            if let Some(sink) = sink.set_locked(true) {
                drop(b);
                sink.lock();
                b = self.0.borrow_mut();
            }
            idx += 1;
        }
        if b.locked == 0 {
            self.unlock_apply();
        }
    }
    pub fn unlock(&self, modified: bool) {
        self.unlock_with(modified, || {});
    }
    pub fn unlock_with(&self, modified: bool, on_modify_completed: impl Fn()) {
        let mut b = self.0.borrow_mut();
        assert!(b.locked != 0, "BindSource : unlock when not locked.");
        b.locked -= 1;
        b.modified |= modified;
        if b.locked != 0 {
            return;
        }
        let modified = b.modified;
        for s in b.sinks.iter_mut() {
            s.modified |= modified;
        }
        b.modified = false;
        if modified {
            on_modify_completed();
        }
        self.unlock_apply();
    }
    fn unlock_apply(&self) {
        let mut b = self.0.borrow_mut();
        if b.locked != 0 {
            return;
        }
        let mut idx = 0;
        loop {
            if let Some(sink) = b.sinks.get_mut(idx) {
                let modified = replace(&mut sink.modified, false);
                if let Some(sink) = sink.set_locked(false) {
                    drop(b);
                    sink.unlock(modified);
                    b = self.0.borrow_mut();
                    if b.locked != 0 {
                        return;
                    }
                }
                idx += 1;
            } else {
                break;
            }
        }
    }
}
impl BindSource for BindSinks {
    fn sinks(&self) -> &BindSinks {
        self
    }
}
impl BindSinkEntry {
    fn set_locked(&mut self, locked: bool) -> Option<Rc<dyn BindSink>> {
        if locked != self.locked {
            if let Some(sink) = self.sink.upgrade() {
                self.locked = locked;
                return Some(sink);
            }
        }
        None
    }
}

pub struct Bindings(Vec<BindingEntry>);
struct BindingEntry {
    src: Rc<dyn BindSource>,
    binding: Binding,
}
impl Bindings {
    pub fn new() -> Self {
        Self(Vec::new())
    }
    pub fn context(&mut self, sink: Rc<dyn BindSink>) -> BindContext {
        let sink_weak = Rc::downgrade(&sink);
        let srcs = &mut self.0;
        let srcs_len = srcs.len();
        BindContext {
            sink,
            sink_weak,
            srcs,
            srcs_len,
        }
    }
}

pub struct BindContext<'a> {
    sink: Rc<dyn BindSink>,
    sink_weak: Weak<dyn BindSink>,
    srcs: &'a mut Vec<BindingEntry>,
    srcs_len: usize,
}
impl<'a> BindContext<'a> {
    pub fn bind(&mut self, src: Rc<dyn BindSource>) {
        if self.srcs_len < self.srcs.len() {
            let e = &mut self.srcs[self.srcs_len];
            if !Rc::ptr_eq(&src, &e.src) {
                let e = replace(e, BindingEntry::bind(src, &self.sink));
                e.unbind(&self.sink_weak);
            }
        } else {
            self.srcs.push(BindingEntry::bind(src, &self.sink));
        }
        self.srcs_len += 1;
    }
}
impl<'a> Drop for BindContext<'a> {
    fn drop(&mut self) {
        let range = self.srcs_len..self.srcs.len();
        for e in self.srcs.drain(range) {
            e.unbind(&self.sink_weak);
        }
    }
}

impl BindingEntry {
    fn bind(src: Rc<dyn BindSource>, sink: &Rc<dyn BindSink>) -> Self {
        let binding = src.bind(sink);
        BindingEntry { src, binding }
    }
    fn unbind(self, sink: &Weak<dyn BindSink>) {
        self.src.unbind(self.binding, sink);
    }
}
