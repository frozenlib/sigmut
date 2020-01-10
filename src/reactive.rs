use std::any::Any;
use std::cell::RefCell;
use std::mem::drop;
use std::ops::Deref;
use std::rc::Rc;

pub mod cell;

use crate::binding::*;

pub trait Re {
    type Item;
    fn get(&self, ctx: &mut BindContext) -> Self::Item;

    fn map<F: Fn(Self::Item) -> U, U>(self, f: F) -> Map<Self, F>
    where
        Self: Sized,
    {
        Map { s: self, f }
    }

    fn cached(self) -> RcReRef<Self::Item>
    where
        Self: Sized + 'static,
    {
        RcReRef(Rc::new(ReCacheData::new(self)))
    }
    fn into_rc(self) -> RcRe<Self::Item>
    where
        Self: Sized + 'static,
    {
        RcRe(Rc::new(self))
    }
}
pub trait ReRef {
    type Item;
    fn borrow(&self, ctx: &mut BindContext) -> Ref<Self::Item>;

    fn map<F: Fn(&Self::Item) -> U, U>(self, f: F) -> MapRef<Self, F>
    where
        Self: Sized,
    {
        MapRef { s: self, f }
    }
    fn map_ref<F: Fn(&Self::Item) -> &U, U>(self, f: F) -> MapRefRef<Self, F>
    where
        Self: Sized,
    {
        MapRefRef { s: self, f }
    }

    fn cloned(self) -> Cloned<Self>
    where
        Self: Sized,
        Self::Item: Clone,
    {
        Cloned(self)
    }

    fn into_rc(self) -> RcReRef<Self::Item>
    where
        Self: Sized + 'static,
    {
        RcReRef(Rc::new(self))
    }
}

pub enum Ref<'a, T> {
    Native(&'a T),
    Cell(std::cell::Ref<'a, T>),
}
impl<'a, T> Ref<'a, T> {
    pub fn map<U>(this: Self, f: impl FnOnce(&T) -> &U) -> Ref<'a, U> {
        match this {
            Ref::Native(x) => Ref::Native(f(x)),
            Ref::Cell(x) => Ref::Cell(std::cell::Ref::map(x, f)),
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

pub trait DynRe<T> {
    fn as_any(self: Rc<Self>) -> Rc<dyn Any>;
    fn dyn_get(&self, this: Rc<dyn Any>, ctx: &mut BindContext) -> T;
}
pub trait DynReRef<T> {
    fn as_any(self: Rc<Self>) -> Rc<dyn Any>;
    fn dyn_borrow(&self, this: Rc<dyn Any>, ctx: &mut BindContext) -> Ref<T>;
}

impl<R: Re + Any> DynRe<R::Item> for R {
    fn as_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
    fn dyn_get(&self, _this: Rc<dyn Any>, ctx: &mut BindContext) -> R::Item {
        Re::get(self, ctx)
    }
}
impl<R: ReRef + Any> DynReRef<R::Item> for R {
    fn as_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
    fn dyn_borrow(&self, _this: Rc<dyn Any>, ctx: &mut BindContext) -> Ref<R::Item> {
        self.borrow(ctx)
    }
}

pub struct RcRe<T>(Rc<dyn DynRe<T>>);
pub struct RcReRef<T>(Rc<dyn DynReRef<T>>);

impl<T> RcRe<T> {}
impl<T> RcReRef<T> {}

impl<T> Re for RcRe<T> {
    type Item = T;
    fn get(&self, ctx: &mut BindContext) -> T {
        self.0.dyn_get(self.0.clone().as_any(), ctx)
    }
    fn into_rc(self) -> RcRe<T> {
        self
    }
}
impl<T> ReRef for RcReRef<T> {
    type Item = T;

    fn borrow(&self, ctx: &mut BindContext) -> Ref<T> {
        self.0.dyn_borrow(self.0.clone().as_any(), ctx)
    }
    fn into_rc(self) -> RcReRef<T> {
        self
    }
}

impl<T, F: Fn(&BindContext) -> T> Re for F {
    type Item = T;
    fn get(&self, ctx: &mut BindContext) -> T {
        self(ctx)
    }
}
impl<S0: Re, S1: Re> Re for (S0, S1) {
    type Item = (S0::Item, S1::Item);
    fn get(&self, ctx: &mut BindContext) -> Self::Item {
        (self.0.get(ctx), self.1.get(ctx))
    }
}

struct ReCacheData<S: Re> {
    src: S,
    value: RefCell<Option<S::Item>>,
    sinks: BindSinks,
    binds: RefCell<Bindings>,
}
impl<S: Re> ReCacheData<S> {
    fn new(src: S) -> Self {
        Self {
            src,
            value: RefCell::new(None),
            sinks: BindSinks::new(),
            binds: RefCell::new(Bindings::new()),
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
        self.sinks.unlock_with(modified, || {
            *self.value.borrow_mut() = None;
        });
    }
}

impl<S: Re + 'static> DynReRef<S::Item> for ReCacheData<S> {
    fn as_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
    fn dyn_borrow(&self, this: Rc<dyn Any>, ctx: &mut BindContext) -> Ref<S::Item> {
        let this = Rc::downcast::<Self>(this).unwrap();
        ctx.bind(this.clone());
        let mut b = self.value.borrow();
        if b.is_none() {
            drop(b);
            *self.value.borrow_mut() =
                Some(self.src.get(&mut self.binds.borrow_mut().context(this)));
            b = self.value.borrow();
        }
        return Ref::Cell(std::cell::Ref::map(b, |x| x.as_ref().unwrap()));
    }
}

pub struct Constant<T>(T);
impl<T> ReRef for Constant<T> {
    type Item = T;
    fn borrow(&self, _ctx: &mut BindContext) -> Ref<Self::Item> {
        Ref::Native(&self.0)
    }
}

pub struct Cloned<S>(S);
impl<S: ReRef> Re for Cloned<S>
where
    S::Item: Clone,
{
    type Item = S::Item;
    fn get(&self, ctx: &mut BindContext) -> Self::Item {
        self.0.borrow(ctx).clone()
    }
}

pub struct Map<S, F> {
    s: S,
    f: F,
}

impl<S: Re, F: Fn(S::Item) -> U, U> Re for Map<S, F> {
    type Item = U;
    fn get(&self, ctx: &mut BindContext) -> Self::Item {
        (self.f)(self.s.get(ctx))
    }
}

pub struct MapRef<S, F> {
    s: S,
    f: F,
}
impl<S: ReRef, F: Fn(&S::Item) -> U, U> Re for MapRef<S, F> {
    type Item = U;
    fn get(&self, ctx: &mut BindContext) -> Self::Item {
        (self.f)(&self.s.borrow(ctx))
    }
}

pub struct MapRefRef<S, F> {
    s: S,
    f: F,
}

impl<S: ReRef, F: Fn(&S::Item) -> &U, U> ReRef for MapRefRef<S, F> {
    type Item = U;
    fn borrow(&self, ctx: &mut BindContext) -> Ref<U> {
        Ref::map(self.s.borrow(ctx), &self.f)
    }
}
