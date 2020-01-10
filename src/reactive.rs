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

    fn cached(self) -> RcReRef<Self::Item>
    where
        Self: Sized + 'static,
    {
        RcReRef(Rc::new(ReCacheData::new(self)))
    }
}
pub trait ReRef {
    type Item;
    fn borrow(&self, ctx: &mut BindContext) -> Ref<Self::Item>;
}

pub trait ImplRe: Re<Item = <Self as ImplRe>::ImplItem> {
    type ImplItem;
}
pub trait ImplReRef: ReRef<Item = <Self as ImplReRef>::ImplItem> {
    type ImplItem;
}

impl<R: ImplRe> ReRef for R {
    type Item = R::ImplItem;
    fn borrow(&self, ctx: &mut BindContext) -> Ref<Self::Item> {
        Ref::Value(self.get(ctx))
    }
}
impl<R: ImplReRef> Re for R
where
    R::ImplItem: Clone,
{
    type Item = R::ImplItem;
    fn get(&self, ctx: &mut BindContext) -> Self::Item {
        self.borrow(ctx).take_or_clone()
    }
}

pub enum Ref<'a, T> {
    Value(T),
    Ref(&'a T),
    RefCell(std::cell::Ref<'a, T>),
}
impl<'a, T> Ref<'a, T> {
    pub fn take_or_clone(self) -> T
    where
        T: Clone,
    {
        match self {
            Ref::Value(x) => x,
            x => (*x).clone(),
        }
    }
    pub fn try_take(self) -> Option<T> {
        match self {
            Ref::Value(x) => Some(x),
            _ => None,
        }
    }
}

impl<'a, T> Deref for Ref<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        match self {
            Ref::Value(x) => x,
            Ref::Ref(x) => x,
            Ref::RefCell(x) => x,
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

impl<T> Re for RcRe<T> {
    type Item = T;
    fn get(&self, ctx: &mut BindContext) -> T {
        self.0.dyn_get(self.0.clone().as_any(), ctx)
    }
}

impl<T: Clone> Re for RcReRef<T> {
    type Item = T;
    fn get(&self, ctx: &mut BindContext) -> T {
        self.borrow(ctx).take_or_clone()
    }
}
impl<T> ReRef for RcReRef<T> {
    type Item = T;

    fn borrow(&self, ctx: &mut BindContext) -> Ref<T> {
        self.0.dyn_borrow(self.0.clone().as_any(), ctx)
    }
}

struct ReCacheData<S: Re> {
    src: S,
    value: RefCell<Option<S::Item>>,
    sinks: BindSinks,
    srcs: RefCell<Bindings>,
}
impl<S: Re> ReCacheData<S> {
    fn new(src: S) -> Self {
        Self {
            src,
            value: RefCell::new(None),
            sinks: BindSinks::new(),
            srcs: RefCell::new(Bindings::new()),
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
                Some(self.src.get(&mut self.srcs.borrow_mut().context(this)));
            b = self.value.borrow();
        }
        return Ref::RefCell(std::cell::Ref::map(b, |x| x.as_ref().unwrap()));
    }
}

pub struct Constant<T>(T);
impl<T: Clone> Re for Constant<T> {
    type Item = T;
    fn get(&self, _ctx: &mut BindContext) -> Self::Item {
        self.0.clone()
    }
}
impl<T> ReRef for Constant<T> {
    type Item = T;
    fn borrow(&self, _ctx: &mut BindContext) -> Ref<Self::Item> {
        Ref::Ref(&self.0)
    }
}
