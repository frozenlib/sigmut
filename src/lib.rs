use std::cell::RefCell;
use std::mem::{drop, replace};
use std::rc::{Rc, Weak};

pub trait BindSource {
    fn bind(&self, sink: &Rc<dyn BindSink>) -> Binding;
    fn unbind(&self, binding: Binding, sink: &Weak<dyn BindSink>);
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
    fn bind(&self, sink: &Rc<dyn BindSink>) -> Binding {
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
    fn unbind(&self, binding: Binding, sink: &Weak<dyn BindSink>) {
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
}
impl BindSink for BindSinks {
    fn lock(&self) {
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
    fn unlock(&self, modified: bool) {
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
        self.unlock_apply();
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
