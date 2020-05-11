mod cell;

pub use self::cell::*;
use crate::bind::*;
use std::{
    any::Any,
    cell::RefCell,
    ops::Deref,
    rc::{Rc, Weak},
};

/// The context of `Re::get` and `ReRef::borrow`.
pub struct ReactiveContext<'a> {
    sink: Weak<dyn BindSink>,
    bindings: &'a mut Vec<Binding>,
}
impl<'a> ReactiveContext<'a> {
    pub fn new(sink: &Rc<impl BindSink + 'static>, bindings: &'a mut Vec<Binding>) -> Self {
        debug_assert!(bindings.is_empty());
        Self {
            sink: Rc::downgrade(sink) as Weak<dyn BindSink>,
            bindings,
        }
    }
    pub fn bind(&mut self, src: Rc<impl BindSource>) {
        self.bindings.push(src.bind(self.sink.clone()));
    }
}

/// A wrapper type for an immutably borrowed value from a `ReRef`.
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

trait DynRe: 'static {
    type Item;
    fn dyn_get(&self, ctx: &mut ReactiveContext) -> Self::Item;
}
trait DynReSource: 'static {
    type Item;
    fn dyn_get(self: Rc<Self>, ctx: &mut ReactiveContext) -> Self::Item;
}

trait DynReRef: 'static {
    type Item;
    fn dyn_borrow(&self, ctx: &mut ReactiveContext) -> Ref<Self::Item>;
}
trait DynReRefSource: Any + 'static {
    type Item;

    fn dyn_borrow<'a>(
        &'a self,
        rc_self: &Rc<dyn DynReRefSource<Item = Self::Item>>,
        ctx: &mut ReactiveContext,
    ) -> Ref<'a, Self::Item>;
    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any>;

    fn downcast(rc_self: &Rc<dyn DynReRefSource<Item = Self::Item>>) -> Rc<Self>
    where
        Self: Sized,
    {
        rc_self.clone().as_rc_any().downcast::<Self>().unwrap()
    }
}

pub struct Re<T: 'static>(ReData<T>);

enum ReData<T: 'static> {
    Dyn(Rc<dyn DynRe<Item = T>>),
    DynSource(Rc<dyn DynReSource<Item = T>>),
}

pub struct ReRef<T: 'static>(ReRefData<T>);

enum ReRefData<T: 'static> {
    Constant(Rc<T>),
    Dyn(Rc<dyn DynReRef<Item = T>>),
    DynSource(Rc<dyn DynReRefSource<Item = T>>),
}

impl<T: 'static> Re<T> {
    pub fn get(&self, ctx: &mut ReactiveContext) -> T {
        match &self.0 {
            ReData::Dyn(rc) => rc.dyn_get(ctx),
            ReData::DynSource(rc) => rc.clone().dyn_get(ctx),
        }
    }

    pub fn from_get(get: impl Fn(&mut ReactiveContext) -> T + 'static) -> Self {
        struct ReFn<F>(F);
        impl<F: Fn(&mut ReactiveContext) -> T + 'static, T> DynRe for ReFn<F> {
            type Item = T;
            fn dyn_get(&self, ctx: &mut ReactiveContext) -> Self::Item {
                (self.0)(ctx)
            }
        }
        Self::from_dyn(ReFn(get))
    }
    fn from_dyn(inner: impl DynRe<Item = T>) -> Self {
        Self(ReData::Dyn(Rc::new(inner)))
    }
    fn from_dyn_source(inner: impl DynReSource<Item = T>) -> Self {
        Self(ReData::DynSource(Rc::new(inner)))
    }

    pub fn map<U: 'static>(self, f: impl Fn(T) -> U + 'static) -> Re<U> {
        Re::from_get(move |ctx| f(self.get(ctx)))
    }

    pub fn cached(self) -> ReRef<T> {
        ReRef::from_dyn_source(Cached::new(self))
    }
}

impl<T: 'static> ReRef<T> {
    pub fn borrow(&self, ctx: &mut ReactiveContext) -> Ref<T> {
        match &self.0 {
            ReRefData::Constant(rc) => Ref::Native(&rc),
            ReRefData::Dyn(rc) => rc.dyn_borrow(ctx),
            ReRefData::DynSource(rc) => rc.dyn_borrow(&rc, ctx),
        }
    }

    pub fn constant(value: T) -> Self {
        Self(ReRefData::Constant(Rc::new(value)))
    }
    fn from_dyn(inner: impl DynReRef<Item = T>) -> Self {
        Self(ReRefData::Dyn(Rc::new(inner)))
    }
    fn from_dyn_source(inner: impl DynReRefSource<Item = T>) -> Self {
        Self(ReRefData::DynSource(Rc::new(inner)))
    }
}

// pub trait IntoRe<T> {
//     fn into_re(self) -> Re<T>;
// }
// impl<T> IntoRe<T> for T {
//     fn into_re(self) -> Re<T> {
//         Re::constant(self)
//     }
// }
// impl<T> IntoRe<T> for Re<T> {
//     fn into_re(self) -> Re<T> {
//         self
//     }
// }
// pub trait IntoReRef<T> {
//     fn into_re_ref(self) -> ReRef<T>;
// }
// impl<T> IntoReRef<T> for T {
//     fn into_re_ref(self) -> ReRef<T> {
//         ReRef::constant(self)
//     }
// }
// impl<T> IntoReRef<T> for ReRef<T> {
//     fn into_re_ref(self) -> ReRef<T> {
//         self
//     }
// }

struct Cached<T: 'static> {
    s: Re<T>,
    sinks: BindSinks,
    state: RefCell<CachedState<T>>,
}

struct CachedState<T> {
    value: Option<T>,
    bindings: Vec<Binding>,
}
impl<T> Cached<T> {
    fn new(s: Re<T>) -> Self {
        Cached {
            s,
            sinks: BindSinks::new(),
            state: RefCell::new(CachedState {
                value: None,
                bindings: Vec::new(),
            }),
        }
    }
}
impl<T: 'static> DynReRefSource for Cached<T> {
    type Item = T;

    fn dyn_borrow<'a>(
        &'a self,
        rc_self: &Rc<dyn DynReRefSource<Item = Self::Item>>,
        ctx: &mut ReactiveContext,
    ) -> Ref<'a, Self::Item> {
        let rc_self = Self::downcast(rc_self);
        ctx.bind(rc_self.clone());
        let mut s = self.state.borrow();
        if s.value.is_none() {
            drop(s);
            {
                let mut s = self.state.borrow_mut();
                let mut ctx = ReactiveContext::new(&rc_self, &mut s.bindings);
                s.value = Some(self.s.get(&mut ctx));
            }
            s = self.state.borrow();
        }
        return Ref::map(Ref::Cell(s), |o| o.value.as_ref().unwrap());
    }

    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
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
            s.bindings.clear();
            self.sinks.notify_with(ctx);
        }
    }
}

/*

pub fn from_borrow<T, F, U>(this: T, borrow: F) -> impl ReactiveRef<Item = U>
where
    T: 'static,
    for<'a> F: Fn(&'a T, &mut ReactiveContext) -> Ref<'a, U> + 'static,
    U: 'static,
{
    struct FnReactiveRef<T, F> {
        this: T,
        borrow: F,
    }
    impl<T, F, U> ReactiveRef for FnReactiveRef<T, F>
    where
        T: 'static,
        for<'a> F: Fn(&'a T, &mut ReactiveContext) -> Ref<'a, U> + 'static,
        U: 'static,
    {
        type Item = U;
        fn borrow(&self, ctx: &mut ReactiveContext) -> Ref<U> {
            (self.borrow)(&self.this, ctx)
        }
    }
    FnReactiveRef { this, borrow }
}
*/
