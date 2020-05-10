use crate::*;
use std::any::Any;
use std::{cell::RefCell, rc::Rc};

pub trait InnerRe: 'static {
    type Item;

    fn dyn_get(self: Rc<Self>, ctx: &mut ReactiveContext) -> Self::Item;
}
pub trait InnerReRef: 'static {
    type Item;

    fn dyn_borrow<'a>(
        &'a self,
        rc_self: &'a dyn Any,
        ctx: &mut ReactiveContext,
    ) -> Ref<'a, Self::Item>;
    fn downcast(rc_self: &dyn Any) -> &Rc<Self>
    where
        Self: Sized,
    {
        rc_self.downcast_ref().unwrap()
    }
}

impl<B: Reactive> InnerRe for B {
    type Item = B::Item;
    fn dyn_get(self: Rc<Self>, ctx: &mut ReactiveContext) -> Self::Item {
        self.get(ctx)
    }
}
impl<B: ReactiveRef> InnerReRef for B {
    type Item = B::Item;

    fn dyn_borrow<'a>(
        &'a self,
        _rc_self: &'a dyn Any,
        ctx: &mut ReactiveContext,
    ) -> Ref<'a, Self::Item> {
        self.borrow(ctx)
    }
}

type RcRe<T> = Rc<dyn InnerRe<Item = T>>;
type RcReRef<T> = Rc<dyn InnerReRef<Item = T>>;

impl<T: 'static> Reactive for RcRe<T> {
    type Item = T;

    fn get(&self, ctx: &mut ReactiveContext) -> Self::Item {
        self.clone().dyn_get(ctx)
    }
}
impl<T: 'static> ReactiveRef for RcReRef<T> {
    type Item = T;

    fn borrow(&self, ctx: &mut ReactiveContext) -> Ref<Self::Item> {
        self.dyn_borrow(self, ctx)
    }
}

#[derive(Clone)]
pub enum Re<T: 'static> {
    Constant(T),
    Dyn(Rc<dyn InnerRe<Item = T>>),
}

#[derive(Clone)]
pub enum ReRef<T: 'static> {
    Constant(T),
    Dyn(Rc<dyn InnerReRef<Item = T>>),
}

impl<T: 'static> Re<T> {
    pub fn from_get(get: impl Fn(&mut ReactiveContext) -> T + 'static) -> Self {
        Self::from_inner(make_reactive(get))
    }
    pub fn from_inner(inner: impl InnerRe<Item = T>) -> Self {
        Re::Dyn(Rc::new(inner))
    }

    pub fn map<U: 'static>(self, f: impl Fn(T) -> U + 'static) -> Re<U> {
        match self {
            Re::Constant(value) => Re::Constant(f(value)),
            Re::Dyn(s) => Re::from_get(move |ctx| f(s.get(ctx))),
        }
    }

    pub fn cached(self) -> ReRef<T> {
        match self {
            Re::Constant(value) => ReRef::Constant(value),
            Re::Dyn(s) => ReRef::from_inner(Cached::new(s)),
        }
    }
}
impl<T: 'static> ReRef<T> {
    pub fn from_inner(inner: impl InnerReRef<Item = T>) -> Self {
        ReRef::Dyn(Rc::new(inner))
    }
}

struct Cached<T> {
    s: RcRe<T>,
    sinks: BindSinks,
    state: RefCell<CachedState<T>>,
}

struct CachedState<T> {
    value: Option<T>,
    binds: Vec<Binding>,
}
impl<T> Cached<T> {
    fn new(s: RcRe<T>) -> Self {
        Cached {
            s,
            sinks: BindSinks::new(),
            state: RefCell::new(CachedState {
                value: None,
                binds: Vec::new(),
            }),
        }
    }
}
impl<T: 'static> Cached<T> {
    fn ready(self: &Rc<Self>) {
        let mut s = self.state.borrow_mut();
        let mut ctx = ReactiveContext::new(&self, &mut s.binds);
        s.value = Some(self.s.get(&mut ctx));
    }
    fn borrow<'a>(self: &'a Rc<Self>, ctx: &mut ReactiveContext) -> Ref<'a, T> {
        ctx.bind(self.clone());
        let mut s = self.state.borrow();
        if s.value.is_none() {
            drop(s);
            self.ready();
            s = self.state.borrow();
        }
        return Ref::map(Ref::Cell(s), |o| o.value.as_ref().unwrap());
    }
}
impl<T: 'static> InnerReRef for Cached<T> {
    type Item = T;

    fn dyn_borrow<'a>(
        &'a self,
        rc_self: &'a dyn Any,
        ctx: &mut ReactiveContext,
    ) -> Ref<'a, Self::Item> {
        Self::downcast(rc_self).borrow(ctx)
    }
}
impl<T: 'static> BindSource for Cached<T> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
impl<T: 'static> BindSink for Cached<T> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        let mut s = self.state.borrow_mut();
        if s.value.is_some() {
            s.value = None;
            s.binds.clear();
            self.sinks.notify_with(ctx);
        }
    }
}

pub trait IntoRe<T> {
    fn into_re(self) -> Re<T>;
}
impl<T> IntoRe<T> for T {
    fn into_re(self) -> Re<T> {
        Re::Constant(self)
    }
}
impl<T> IntoRe<T> for Re<T> {
    fn into_re(self) -> Re<T> {
        self
    }
}

pub trait IntoReRef<T> {
    fn into_re_ref(self) -> ReRef<T>;
}
impl<T> IntoReRef<T> for T {
    fn into_re_ref(self) -> ReRef<T> {
        ReRef::Constant(self)
    }
}
impl<T> IntoReRef<T> for ReRef<T> {
    fn into_re_ref(self) -> ReRef<T> {
        self
    }
}
