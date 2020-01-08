use std::cell::RefCell;
use std::mem::{drop, replace};
use std::rc::{Rc, Weak};

pub struct BindSource(Rc<BindSourceCell>);
struct BindSourceCell(RefCell<BindSourceData>);
struct BindSourceData {
    sinks: Vec<BindSinkLink>,
    locked: usize,
}
struct BindSinkLink {
    sink: Weak<dyn BindSink>,
    locked: bool,
}

impl BindSource {
    pub fn new() -> Self {
        Self(Rc::new(BindSourceCell(RefCell::new(BindSourceData {
            sinks: Vec::new(),
            locked: 0,
        }))))
    }
    pub fn lock(&self) -> BindSourceLockGuard {
        self.0.lock();
        BindSourceLockGuard {
            source: &self.0,
            modified: false,
        }
    }
    pub fn as_bind_sink(&self) -> Rc<dyn BindSink> {
        self.0 as Rc<dyn BindSink>
    }
}

impl BindSourceCell {
    fn bind(&self, sink: &Rc<dyn BindSink>) -> usize {
        let mut b = self.0.borrow_mut();
        let locked = b.locked != 0;
        let s = BindSinkLink {
            sink: Rc::downgrade(sink),
            locked,
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
        idx
    }
    fn unbind(&self, idx: usize, sink: &Weak<dyn BindSink>) {
        let mut b = self.0.borrow_mut();
        if let Some(s) = b.sinks.get_mut(idx) {
            if s.sink.ptr_eq(sink) {
                let locked = s.locked;
                s.sink = Weak::<Self>::new();
                s.locked = false;
                if locked {
                    if let Some(sink) = sink.upgrade() {
                        sink.unlock();
                    }
                }
            }
        }
    }
    fn for_each<T>(
        &self,
        init: impl Fn(&mut BindSourceData) -> bool,
        get: impl Fn(&mut BindSinkLink) -> Option<T>,
        apply: impl Fn(T),
    ) {
        let mut b = self.0.borrow_mut();
        init(&mut b);
        let mut idx = 0;
        while let Some(sink) = b.sinks.get_mut(idx) {
            if let Some(value) = get(sink) {
                drop(b);
                apply(value);
                b = self.0.borrow_mut();
            }
            idx += 1;
        }
    }
}
impl BindSink for BindSourceCell {
    fn lock(&self) {
        self.for_each(
            |b| {
                if b.locked == usize::max_value() {
                    panic!("BindSource : too many locked.")
                }
                b.locked += 1;
                true
            },
            |sink| sink.set_locked(true),
            |sink| sink.lock(),
        );
    }
    fn modify(&self) {
        self.for_each(
            |b| {
                assert!(b.locked != 0, "BindSource : modify when not locked.");
                true
            },
            |sink| sink.sink_if_locked(),
            |sink| sink.modify(),
        );
    }
    fn unlock(&self) {
        self.for_each(
            |b| {
                assert!(b.locked != 0, "BindSource : unlock when not locked.");
                b.locked -= 1;
                true
            },
            |sink| sink.set_locked(false),
            |sink| sink.unlock(),
        );
    }
}

impl BindSinkLink {
    fn set_locked(&mut self, locked: bool) -> Option<Rc<dyn BindSink>> {
        if locked != self.locked {
            if let Some(sink) = self.sink.upgrade() {
                self.locked = locked;
                return Some(sink);
            }
        }
        None
    }
    fn sink_if_locked(&mut self) -> Option<Rc<dyn BindSink>> {
        if self.locked {
            self.sink.upgrade()
        } else {
            None
        }
    }
}

pub struct BindSourceLockGuard<'a> {
    source: &'a BindSourceCell,
    modified: bool,
}
impl<'a> BindSourceLockGuard<'a> {
    pub fn modify(&mut self) {
        if self.modified {
            return;
        }
        self.modified = true;
        self.source.modify();
    }
}
impl<'a> Drop for BindSourceLockGuard<'a> {
    fn drop(&mut self) {
        self.source.unlock();
    }
}

trait BindSinkDef {
    type State;
    fn lock(&self, state: &mut Self::State);
    fn modify(&self, state: &mut Self::State);
    fn unlock(&self, state: &mut Self::State);
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
trait BindSink {
    fn lock(&self);
    fn modify(&self);
    fn unlock(&self);
}

pub struct BindContext {}

pub trait Re {
    type Item;

    fn get(&self, ctx: &BindContext) -> Self::Item;
}

pub fn bind<T>(source: impl Re<Item = T>, f: impl Fn(T)) {
    //-> impl Drop {
    let sink = BindSink::new(f);

    unimplemented!()
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
