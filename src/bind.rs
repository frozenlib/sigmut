use crate::event::*;
use std::any::Any;
use std::cell::RefCell;
use std::mem::drop;
use std::ops::Deref;
use std::rc::{Rc, Weak};

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

    fn for_each(self, f: impl Fn(Self::Item) + 'static) -> Unbind {
        Unbind(ForEachData::new(self, f))
    }
}

pub trait RefBind: Sized + 'static {
    type Item;

    fn bind(&self, ctx: &mut BindContext) -> Ref<Self::Item>;
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

pub(crate) struct ForEachData<B, F> {
    b: B,
    f: F,
    binds: RefCell<Vec<Binding>>,
}

impl<B: Bind, F: Fn(B::Item) + 'static> ForEachData<B, F> {
    pub fn new(b: B, f: F) -> Rc<Self> {
        let s = Rc::new(ForEachData {
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
