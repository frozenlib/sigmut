use std::any::Any;
use std::cell::RefCell;
use std::cmp::min;
use std::mem::drop;
use std::mem::replace;
use std::ops::Deref;
use std::rc::{Rc, Weak};

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
    pub fn is_empty(&self) -> bool {
        self.0
            .borrow_mut()
            .sinks
            .iter()
            .all(|x| x.strong_count() == 0)
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

pub struct NotifyContext(RefCell<NotifyContextData>);
struct NotifyContextData {
    ref_count: usize,
    tasks: Vec<Weak<dyn Task>>,
}

pub trait Task: 'static {
    fn run(self: Rc<Self>);
}

impl NotifyContext {
    fn new() -> Self {
        Self(RefCell::new(NotifyContextData {
            ref_count: 0,
            tasks: Vec::new(),
        }))
    }

    pub fn with(f: impl Fn(&NotifyContext)) {
        thread_local!(static CTX: NotifyContext = NotifyContext::new());
        CTX.with(|ctx| {
            ctx.enter();
            f(ctx);
            ctx.leave();
        });
    }
    fn enter(&self) {
        let mut d = self.0.borrow_mut();
        assert!(d.ref_count != usize::max_value());
        d.ref_count += 1;
    }
    fn leave(&self) {
        let mut d = self.0.borrow_mut();
        assert!(d.ref_count != 0);
        if d.ref_count == 1 {
            while let Some(task) = d.tasks.pop() {
                if let Some(task) = Weak::upgrade(&task) {
                    drop(d);
                    task.run();
                    d = self.0.borrow_mut();
                }
            }
        }
        d.ref_count -= 1;
    }

    pub fn spawn(&self, task: Weak<impl Task>) {
        self.0.borrow_mut().tasks.push(task);
    }
}

pub struct BindContext<'a> {
    sink: Weak<dyn BindSink>,
    bindings: &'a mut Vec<Binding>,
}
impl<'a> BindContext<'a> {
    pub fn new(sink: &Rc<impl BindSink + 'static>, bindings: &'a mut Vec<Binding>) -> Self {
        Self {
            sink: Rc::downgrade(sink) as Weak<dyn BindSink>,
            bindings,
        }
    }
    pub fn bind(&mut self, src: Rc<impl BindSource>) {
        self.bindings.push(src.bind(self.sink.clone()));
    }
}

pub trait Bind: Sized + 'static {
    type Item;

    fn bind(&self, ctx: &mut BindContext) -> Self::Item;

    fn cached(self) -> Cached<Self> {
        Cached::new(self)
    }
    fn cached_ne(self) -> CachedNe<Self>
    where
        Self::Item: PartialEq,
    {
        CachedNe::new(self)
    }

    fn for_each(self, f: impl Fn(Self::Item) + 'static) -> Unbind {
        Unbind(ForEachData::new(self, f))
    }

    fn map<F: Fn(Self::Item) -> U, U>(self, f: F) -> Map<Self, F> {
        Map { b: self, f }
    }
}

pub trait RefBind: Sized + 'static {
    type Item;

    fn bind(&self, ctx: &mut BindContext) -> Ref<Self::Item>;

    fn for_each(self, f: impl Fn(&Self::Item) + 'static) -> Unbind {
        Unbind(RefForEachData::new(self, f))
    }

    fn map<F: Fn(&Self::Item) -> U, U>(self, f: F) -> RefMap<Self, F> {
        RefMap { b: self, f }
    }
    fn map_ref<F: Fn(&Self::Item) -> &U, U>(self, f: F) -> RefMapRef<Self, F> {
        RefMapRef { b: self, f }
    }
    fn cloned(self) -> Cloned<Self>
    where
        Self::Item: Clone,
    {
        Cloned(self)
    }
}

pub enum Ref<'a, T> {
    Native(&'a T),
    Cell(std::cell::Ref<'a, T>),
}
impl<'a, T> Ref<'a, T> {
    pub fn map<U>(this: Self, f: impl FnOnce(&T) -> &U) -> Ref<'a, U> {
        use Ref::*;
        match this {
            Native(x) => Native(f(x)),
            Cell(x) => Cell(std::cell::Ref::map(x, f)),
        }
    }
}
impl<'a, T> Deref for Ref<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        match self {
            Ref::Native(x) => x,
            Ref::Cell(x) => x,
        }
    }
}
pub struct Unbind(Rc<dyn Any>);

#[derive(Clone)]
pub struct Cached<B: Bind>(Rc<CachedData<B>>);
struct CachedData<B: Bind> {
    b: B,
    sinks: BindSinks,
    state: RefCell<CachedState<B::Item>>,
}
struct CachedState<T> {
    value: Option<T>,
    binds: Vec<Binding>,
}

impl<B: Bind> Cached<B> {
    pub fn new(b: B) -> Self {
        Self(Rc::new(CachedData {
            b,
            sinks: BindSinks::new(),
            state: RefCell::new(CachedState {
                value: None,
                binds: Vec::new(),
            }),
        }))
    }

    fn ready(&self) {
        let mut s = self.0.state.borrow_mut();
        let mut ctx = BindContext::new(&self.0, &mut s.binds);
        s.value = Some(self.0.b.bind(&mut ctx));
    }
}
impl<B: Bind> RefBind for Cached<B> {
    type Item = B::Item;
    fn bind(&self, ctx: &mut BindContext) -> Ref<Self::Item> {
        ctx.bind(self.0.clone());
        let mut s = self.0.state.borrow();
        if s.value.is_none() {
            drop(s);
            self.ready();
            s = self.0.state.borrow();
        }
        return Ref::map(Ref::Cell(s), |o| o.value.as_ref().unwrap());
    }
}
impl<B: Bind> BindSource for CachedData<B> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
impl<B: Bind> BindSink for CachedData<B> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        let mut s = self.state.borrow_mut();
        if s.value.is_some() {
            s.value = None;
            s.binds.clear();
            self.sinks.notify_with(ctx);
        }
    }
}

#[derive(Clone)]
pub struct CachedNe<B: Bind>(Rc<CachedNeData<B>>);

struct CachedNeData<B: Bind> {
    b: B,
    sinks: BindSinks,
    state: RefCell<CachedNeState<B::Item>>,
}
struct CachedNeState<T> {
    value: Option<T>,
    is_ready: bool,
    binds: Vec<Binding>,
}
impl<B: Bind> CachedNe<B>
where
    B::Item: PartialEq,
{
    pub fn new(b: B) -> Self {
        Self(Rc::new(CachedNeData {
            b,
            sinks: BindSinks::new(),
            state: RefCell::new(CachedNeState {
                value: None,
                is_ready: false,
                binds: Vec::new(),
            }),
        }))
    }
}
impl<B: Bind> RefBind for CachedNe<B>
where
    B::Item: PartialEq,
{
    type Item = B::Item;
    fn bind(&self, ctx: &mut BindContext) -> Ref<Self::Item> {
        let mut s = self.0.state.borrow();
        if s.is_ready {
            drop(s);
            self.0.ready();
            s = self.0.state.borrow();
        }
        ctx.bind(self.0.clone());
        return Ref::map(Ref::Cell(s), |o| o.value.as_ref().unwrap());
    }
}
impl<B: Bind> BindSource for CachedNeData<B>
where
    B::Item: PartialEq,
{
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
impl<B: Bind> BindSink for CachedNeData<B>
where
    B::Item: PartialEq,
{
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        let mut s = self.state.borrow_mut();
        if s.is_ready {
            s.is_ready = false;
            s.binds.clear();
            if !self.sinks.is_empty() {
                ctx.spawn(Rc::downgrade(&self));
            }
        }
    }
}
impl<B: Bind> CachedNeData<B>
where
    B::Item: PartialEq,
{
    fn ready(self: &Rc<Self>) {
        let mut s = self.state.borrow_mut();
        let mut ctx = BindContext::new(&self, &mut s.binds);
        let value = self.b.bind(&mut ctx);
        if s.value.as_ref() != Some(&value) {
            s.value = Some(value);
            drop(s);
            self.sinks.notify();
        }
    }
}
impl<B: Bind> Task for CachedNeData<B>
where
    B::Item: PartialEq,
{
    fn run(self: Rc<Self>) {
        self.ready();
    }
}

struct ForEachData<B, F> {
    b: B,
    f: F,
    binds: RefCell<Vec<Binding>>,
}

impl<B: Bind, F: Fn(B::Item) + 'static> ForEachData<B, F> {
    fn new(b: B, f: F) -> Rc<Self> {
        let s = Rc::new(Self {
            b,
            f,
            binds: RefCell::new(Vec::new()),
        });
        s.next();
        s
    }

    fn next(self: &Rc<Self>) {
        let mut b = self.binds.borrow_mut();
        let mut ctx = BindContext::new(&self, &mut b);
        (self.f)(self.b.bind(&mut ctx));
    }
}
impl<B: Bind, F: Fn(B::Item) + 'static> BindSink for ForEachData<B, F> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        ctx.spawn(Rc::downgrade(&self))
    }
}
impl<B: Bind, F: Fn(B::Item) + 'static> Task for ForEachData<B, F> {
    fn run(self: Rc<Self>) {
        self.next();
    }
}

struct RefForEachData<B, F> {
    b: B,
    f: F,
    binds: RefCell<Vec<Binding>>,
}

impl<B: RefBind, F: Fn(&B::Item) + 'static> RefForEachData<B, F> {
    fn new(b: B, f: F) -> Rc<Self> {
        let s = Rc::new(Self {
            b,
            f,
            binds: RefCell::new(Vec::new()),
        });
        s.next();
        s
    }

    fn next(self: &Rc<Self>) {
        let mut b = self.binds.borrow_mut();
        let mut ctx = BindContext::new(&self, &mut b);
        (self.f)(&self.b.bind(&mut ctx));
    }
}
impl<B: RefBind, F: Fn(&B::Item) + 'static> BindSink for RefForEachData<B, F> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        ctx.spawn(Rc::downgrade(&self))
    }
}
impl<B: RefBind, F: Fn(&B::Item) + 'static> Task for RefForEachData<B, F> {
    fn run(self: Rc<Self>) {
        self.next();
    }
}

pub struct Map<B, F> {
    b: B,
    f: F,
}
impl<B: Bind, F: Fn(B::Item) -> U + 'static, U> Bind for Map<B, F> {
    type Item = U;

    fn bind(&self, ctx: &mut BindContext) -> Self::Item {
        (self.f)(self.b.bind(ctx))
    }
}

pub struct RefMap<B, F> {
    b: B,
    f: F,
}
impl<B: RefBind, F: Fn(&B::Item) -> U + 'static, U> Bind for RefMap<B, F> {
    type Item = U;

    fn bind(&self, ctx: &mut BindContext) -> Self::Item {
        (self.f)(&self.b.bind(ctx))
    }
}

pub struct RefMapRef<B, F> {
    b: B,
    f: F,
}
impl<B: RefBind, F: Fn(&B::Item) -> &U + 'static, U> RefBind for RefMapRef<B, F> {
    type Item = U;

    fn bind(&self, ctx: &mut BindContext) -> Ref<Self::Item> {
        Ref::map(self.b.bind(ctx), &self.f)
    }
}

pub struct Cloned<B>(B);
impl<B: RefBind> Bind for Cloned<B>
where
    B::Item: Clone,
{
    type Item = B::Item;
    fn bind(&self, ctx: &mut BindContext) -> Self::Item {
        self.0.bind(ctx).clone()
    }
}

pub fn constant<T>(value: T) -> Constant<T> {
    Constant(value)
}

#[derive(Clone)]
pub struct Constant<T: 'static>(T);

impl<T> RefBind for Constant<T> {
    type Item = T;
    fn bind(&self, _: &mut BindContext) -> Ref<Self::Item> {
        Ref::Native(&self.0)
    }
}

impl<F: Fn(&BindContext) -> T + 'static, T> Bind for F {
    type Item = T;
    fn bind(&self, ctx: &mut BindContext) -> Self::Item {
        self(ctx)
    }
}

impl<F: Fn(&BindContext) -> &'static T + 'static, T: 'static> RefBind for F {
    type Item = T;
    fn bind(&self, ctx: &mut BindContext) -> Ref<Self::Item> {
        Ref::Native(self(ctx))
    }
}
