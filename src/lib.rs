use std::cell::RefCell;
use std::mem::{drop, replace};
use std::rc::{Rc, Weak};

pub struct BindSource(RefCell<BindSourceData>);
struct BindSourceData {
    sinks: Vec<BindSinkEntry>,
    locked: usize,
    modified: bool,
}
struct BindSinkEntry {
    sink: Weak<dyn BindSink>,
    locked: bool,
    modified: bool,
}
pub struct Binding {
    idx: usize,
}
pub trait BindSink {
    fn lock(&self);
    fn unlock(&self, modified: bool);
}

impl BindSource {
    pub fn new() -> Self {
        Self(RefCell::new(BindSourceData {
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
impl BindSink for BindSource {
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

// pub struct BindSink<F>(Rc<RefCell<BindSinkData<F>>>);
// struct BindSinkData<F> {
//     f: F,
//     src: Vec<BindSourceLink>,
//     src_len: usize,
// }

// impl<F> BindSink<F> {
//     pub fn new(f: F) -> Self {
//         Self(Rc::new(RefCell::new(BindSinkData {
//             f,
//             src: Vec::new(),
//             src_len: 0,
//         })))
//     }
// }

pub struct BindContext {}

pub trait Re {
    type Item;

    fn get(&self, ctx: &BindContext) -> Self::Item;
}

// use std::future::Future;

// pub struct Observable<F>(F);
// pub struct Observer<F>(F);

// pub trait Observer {
//     type Input;
//     type Subscription: Future<Output = ()>;
//     fn on_next(&self, value: &Self::Input) -> Self::Subscription;
// }

// pub trait Observable<O: Observer<Input = Self::Output>> {
//     type Output;
//     type Subscription: Future<Output = ()>;
//     fn subscribe(&mut self, observer: O) -> Self::Subscription;
// }

// mod observable {
//     use super::*;
//     pub struct Map<I, F> {
//         i: I,
//         f: F,
//     }
//     impl<I, B, F, O> Observable<O> for Map<I, F>
//     where
//         I: Observable<MapObserver<O, F>>,
//         F: Fn(&I::Output) -> B,
//     {
//         type Output = B;
//         type SubScription = MapSubscription;

//         fn subscribe(&mut self, observable: O) -> MapSubscription {
//             todo!()
//         }
//     }
//     pub struct MapSubscription {}

//     pub struct MapObserver<O, F> {
//         o: O,
//         f: F,
//     }
//     impl<O, F> Observer for MapObserver<O, F> {}
// }
