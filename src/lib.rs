use std::any::Any;
use std::cell::{Ref, RefCell, RefMut};
use std::mem::ManuallyDrop;
use std::mem::{drop, replace};
use std::ops::{Deref, DerefMut};
use std::rc::{Rc, Weak};

pub trait BindSource {
    fn bind_sinks(&self) -> &BindSinks;

    fn bind(&self, sink: &Rc<dyn BindSink>) -> Binding {
        self.bind_sinks().bind(sink)
    }
    fn unbind(&self, binding: Binding, sink: &Weak<dyn BindSink>) {
        self.bind_sinks().unbind(binding, sink);
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
    fn bind_sinks(&self) -> &BindSinks {
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

pub trait Re {
    type Item;
    fn get(&self, ctx: &mut ReContext) -> Self::Item;
}
pub trait ReRef {
    type Item;
    fn borrow(&self, ctx: &mut ReContext) -> ReBorrow<Self::Item>;
}
pub enum ReBorrow<'a, T> {
    Value(T),
    Ref(&'a T),
    RefCell(Ref<'a, T>),
}
impl<'a, T> ReBorrow<'a, T> {
    pub fn take_or_clone(self) -> T
    where
        T: Clone,
    {
        match self {
            ReBorrow::Value(x) => x,
            x => (*x).clone(),
        }
    }
    pub fn try_unwrap(self) -> Option<T> {
        match self {
            ReBorrow::Value(x) => Some(x),
            _ => None,
        }
    }
}

impl<'a, T> Deref for ReBorrow<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        match self {
            ReBorrow::Value(x) => x,
            ReBorrow::Ref(x) => x,
            ReBorrow::RefCell(x) => x,
        }
    }
}

pub struct BindSources(Vec<BindSourceEntry>);
struct BindSourceEntry {
    src: Rc<dyn BindSource>,
    binding: Binding,
}

pub struct ReContext<'a> {
    sink: Rc<dyn BindSink>,
    sink_weak: Weak<dyn BindSink>,
    srcs: &'a mut Vec<BindSourceEntry>,
    srcs_len: usize,
}
impl BindSources {
    pub fn new() -> Self {
        Self(Vec::new())
    }
    pub fn context(&mut self, sink: Rc<dyn BindSink>) -> ReContext {
        let sink_weak = Rc::downgrade(&sink);
        let srcs = &mut self.0;
        let srcs_len = srcs.len();
        ReContext {
            sink,
            sink_weak,
            srcs,
            srcs_len,
        }
    }
}
impl<'a> ReContext<'a> {
    pub fn bind(&mut self, src: Rc<dyn BindSource>) {
        if self.srcs_len < self.srcs.len() {
            let e = &mut self.srcs[self.srcs_len];
            if !Rc::ptr_eq(&src, &e.src) {
                let e = replace(e, BindSourceEntry::bind(src, &self.sink));
                e.unbind(&self.sink_weak);
            }
        } else {
            self.srcs.push(BindSourceEntry::bind(src, &self.sink));
        }
        self.srcs_len += 1;
    }
}
impl<'a> Drop for ReContext<'a> {
    fn drop(&mut self) {
        let range = self.srcs_len..self.srcs.len();
        for e in self.srcs.drain(range) {
            e.unbind(&self.sink_weak);
        }
    }
}

impl BindSourceEntry {
    fn bind(src: Rc<dyn BindSource>, sink: &Rc<dyn BindSink>) -> Self {
        let binding = src.bind(sink);
        BindSourceEntry { src, binding }
    }
    fn unbind(self, sink: &Weak<dyn BindSink>) {
        self.src.unbind(self.binding, sink);
    }
}

#[derive(Clone)]
pub struct ReCell<T>(Rc<ReCellData<T>>);
struct ReCellData<T> {
    value: RefCell<T>,
    sinks: BindSinks,
}

impl<T> ReCell<T> {
    pub fn set(&self, value: T) {
        *self.borrow_mut() = value;
    }
    pub fn borrow_mut(&self) -> ReCellRefMut<T> {
        ReCellRefMut {
            b: ManuallyDrop::new(self.0.value.borrow_mut()),
            sinks: &self.0.sinks,
            modified: false,
        }
    }
    pub fn lock(&self) -> ReCellLockGuard<T> {
        self.0.sinks.lock();
        ReCellLockGuard(self)
    }
}

impl<T: Clone + 'static> Re for ReCell<T> {
    type Item = T;
    fn get(&self, ctx: &mut ReContext) -> Self::Item {
        self.borrow(ctx).clone()
    }
}
impl<T: 'static> ReRef for ReCell<T> {
    type Item = T;
    fn borrow(&self, ctx: &mut ReContext) -> ReBorrow<T> {
        ctx.bind(self.0.clone());
        ReBorrow::RefCell(self.0.value.borrow())
    }
}

impl<T> BindSource for ReCellData<T> {
    fn bind_sinks(&self) -> &BindSinks {
        &self.sinks
    }
}

pub struct ReCellLockGuard<'a, T>(&'a ReCell<T>);
impl<'a, T> Deref for ReCellLockGuard<'a, T> {
    type Target = ReCell<T>;
    fn deref(&self) -> &ReCell<T> {
        &self.0
    }
}
impl<'a, T> Drop for ReCellLockGuard<'a, T> {
    fn drop(&mut self) {
        (self.0).0.sinks.unlock(false);
    }
}

pub struct ReCellRefMut<'a, T> {
    b: ManuallyDrop<RefMut<'a, T>>,
    sinks: &'a BindSinks,
    modified: bool,
}

impl<'a, T> Deref for ReCellRefMut<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.b
    }
}
impl<'a, T> DerefMut for ReCellRefMut<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.modified = true;
        &mut self.b
    }
}
impl<'a, T> Drop for ReCellRefMut<'a, T> {
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.b);
        }
        if self.modified {
            self.sinks.lock();
            self.sinks.unlock(true);
        }
    }
}

pub trait DynReRef<T> {
    fn as_any(self: Rc<Self>) -> Rc<dyn Any>;
    fn borrow(&self, this: Rc<dyn Any>, ctx: &mut ReContext) -> ReBorrow<T>;
}

pub struct ReCache<T>(Rc<dyn DynReRef<T>>);
struct ReCacheData<S: Re> {
    src: S,
    value: RefCell<Option<S::Item>>,
    sinks: BindSinks,
    srcs: RefCell<BindSources>,
}

impl<T> ReCache<T> {
    pub fn from_re_ref(src: impl Re<Item = T> + 'static) -> Self {
        ReCache(Rc::new(ReCacheData::new(src)))
    }
}
impl<T> ReRef for ReCache<T> {
    type Item = T;

    fn borrow(&self, ctx: &mut ReContext) -> ReBorrow<T> {
        self.0.borrow(self.0.clone().as_any(), ctx)
    }
}

impl<S: Re> ReCacheData<S> {
    fn new(src: S) -> Self {
        Self {
            src,
            value: RefCell::new(None),
            sinks: BindSinks::new(),
            srcs: RefCell::new(BindSources::new()),
        }
    }
}
impl<S: Re> BindSource for ReCacheData<S> {
    fn bind_sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
impl<S: Re> BindSink for ReCacheData<S> {
    fn lock(&self) {
        self.sinks.lock();
    }
    fn unlock(&self, modified: bool) {
        self.sinks.unlock_with(modified, || unimplemented!());
    }
}

impl<S: Re + 'static> DynReRef<S::Item> for ReCacheData<S> {
    fn as_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
    fn borrow(&self, this: Rc<dyn Any>, ctx: &mut ReContext) -> ReBorrow<S::Item> {
        let this = Rc::downcast::<Self>(this).unwrap();
        ctx.bind(this.clone());
        let mut b = self.value.borrow();
        if b.is_none() {
            drop(b);
            *self.value.borrow_mut() =
                Some(self.src.get(&mut self.srcs.borrow_mut().context(this)));
            b = self.value.borrow();
        }
        return ReBorrow::RefCell(Ref::map(b, |x| x.as_ref().unwrap()));
    }
}
