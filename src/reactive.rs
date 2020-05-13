mod cached;
mod cell;

pub use self::cell::*;
use crate::bind::*;
use futures::Future;
use std::{
    any::Any,
    cell::Ref,
    cell::RefCell,
    rc::{Rc, Weak},
};

/// The context of `Re::get` and `ReBorrow::borrow`.
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

trait DynRe: 'static {
    type Item;
    fn dyn_get(&self, ctx: &mut ReactiveContext) -> Self::Item;
}
trait DynReSource: 'static {
    type Item;
    fn dyn_get(self: Rc<Self>, ctx: &mut ReactiveContext) -> Self::Item;
}

trait DynReBorrow: 'static {
    type Item;
    fn dyn_borrow(&self, ctx: &mut ReactiveContext) -> Ref<Self::Item>;
}
trait DynReBorrowSource: Any + 'static {
    type Item;

    fn dyn_borrow(
        &self,
        rc_self: &Rc<dyn DynReBorrowSource<Item = Self::Item>>,
        ctx: &mut ReactiveContext,
    ) -> Ref<Self::Item>;
    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any>;

    fn downcast(rc_self: &Rc<dyn DynReBorrowSource<Item = Self::Item>>) -> Rc<Self>
    where
        Self: Sized,
    {
        rc_self.clone().as_rc_any().downcast::<Self>().unwrap()
    }
}

trait DynReRef: 'static {
    type Item;
    fn dyn_with(&self, ctx: &mut ReactiveContext, f: &mut dyn FnMut(&Self::Item));
}

pub struct Unbind(Rc<dyn Any>);

pub struct Re<T: 'static>(ReData<T>);

enum ReData<T: 'static> {
    Dyn(Rc<dyn DynRe<Item = T>>),
    DynSource(Rc<dyn DynReSource<Item = T>>),
}

pub struct ReBorrow<T: 'static>(ReBorrowData<T>);

enum ReBorrowData<T: 'static> {
    Dyn(Rc<dyn DynReBorrow<Item = T>>),
    DynSource(Rc<dyn DynReBorrowSource<Item = T>>),
}

pub struct ReRef<T: 'static>(ReRefData<T>);
enum ReRefData<T: 'static> {
    Constant(T),
    Re(Re<T>),
    ReBorrow(ReBorrow<T>),
    ReRef(Rc<dyn DynReRef<Item = T>>),
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
    pub fn flat_map<U>(self, f: impl Fn(T) -> Re<U> + 'static) -> Re<U> {
        self.map(f).flatten()
    }
    // pub fn map_async<Fut: Future + 'static>(
    //     self,
    //     f: impl Fn(B::Item) -> Fut + 'static,
    // ) -> RefBindExt<impl ReactiveRef<Item = Poll<Fut::Output>>> {
    //     RefBindExt(MapAsync::new(self.map(f)))
    // }

    pub fn cached(self) -> ReBorrow<T> {
        ReBorrow::from_dyn_source(self::cached::Cached::new(self))
    }

    pub fn dedup_by(self, eq: impl Fn(&T, &T) -> bool + 'static) -> ReBorrow<T> {
        ReBorrow::from_dyn_source(DedupBy::new(self, eq))
    }
    pub fn dedup_by_key<K: PartialEq>(self, to_key: impl Fn(&T) -> K + 'static) -> ReBorrow<T> {
        self.dedup_by(move |l, r| to_key(l) == to_key(r))
    }

    pub fn dedup(self) -> ReBorrow<T>
    where
        T: PartialEq,
    {
        self.dedup_by(|l, r| l == r)
    }

    pub fn for_each(self, f: impl Fn(T) + 'static) -> Unbind {
        Unbind(ForEach::new(self, f))
    }
    pub fn for_each_by<U: 'static>(
        self,
        attach: impl Fn(T) -> U + 'static,
        detach: impl Fn(U) + 'static,
    ) -> Unbind {
        Unbind(ForEachBy::new(self, attach, detach))
    }
    pub fn for_each_async_with<Fut, SpawnFn, U>(
        self,
        f: impl Fn(T) -> Fut + 'static,
        spawn: impl Fn(Fut) -> U + 'static,
    ) -> Unbind
    where
        Fut: Future<Output = ()> + 'static,
        U: 'static,
    {
        self.for_each_by(move |value| spawn(f(value)), move |_| {})
    }
}
impl<T: 'static> Re<Re<T>> {
    pub fn flatten(self) -> Re<T> {
        Re::from_get(move |ctx| self.get(ctx).get(ctx))
    }
}

impl<T: 'static> ReBorrow<T> {
    pub fn borrow(&self, ctx: &mut ReactiveContext) -> Ref<T> {
        match &self.0 {
            ReBorrowData::Dyn(rc) => rc.dyn_borrow(ctx),
            ReBorrowData::DynSource(rc) => rc.dyn_borrow(&rc, ctx),
        }
    }

    pub fn constant(value: T) -> Self {
        Self::from_borrow(RefCell::new(value), |cell, _ctx| cell.borrow())
    }
    pub fn from_borrow<S, F>(this: S, borrow: F) -> Self
    where
        S: 'static,
        for<'a> F: Fn(&'a S, &mut ReactiveContext) -> Ref<'a, T> + 'static,
    {
        struct ReBorrowFn<S, F> {
            this: S,
            borrow: F,
        }
        impl<T, S, F> DynReBorrow for ReBorrowFn<S, F>
        where
            T: 'static,
            S: 'static,
            for<'a> F: Fn(&'a S, &mut ReactiveContext) -> Ref<'a, T> + 'static,
        {
            type Item = T;
            fn dyn_borrow(&self, ctx: &mut ReactiveContext) -> Ref<T> {
                (self.borrow)(&self.this, ctx)
            }
        }
        Self::from_dyn(ReBorrowFn { this, borrow })
    }

    fn from_dyn(inner: impl DynReBorrow<Item = T>) -> Self {
        Self(ReBorrowData::Dyn(Rc::new(inner)))
    }
    fn from_dyn_source(inner: impl DynReBorrowSource<Item = T>) -> Self {
        Self(ReBorrowData::DynSource(Rc::new(inner)))
    }
}
impl<T: 'static> ReRef<T> {
    pub fn with<U>(&self, ctx: &mut ReactiveContext, f: impl Fn(&T) -> U) -> U {
        match &self.0 {
            ReRefData::Constant(value) => f(value),
            ReRefData::Re(rc) => f(&rc.get(ctx)),
            ReRefData::ReBorrow(rc) => f(&rc.borrow(ctx)),
            ReRefData::ReRef(rc) => {
                let mut output = None;
                rc.dyn_with(ctx, &mut |value| output = Some(f(value)));
                output.unwrap()
            }
        }
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
// pub trait IntoReBorrow<T> {
//     fn into_re_ref(self) -> ReBorrow<T>;
// }
// impl<T> IntoReBorrow<T> for T {
//     fn into_re_ref(self) -> ReBorrow<T> {
//         ReBorrow::constant(self)
//     }
// }
// impl<T> IntoReBorrow<T> for ReBorrow<T> {
//     fn into_re_ref(self) -> ReBorrow<T> {
//         self
//     }
// }

struct DedupBy<T: 'static, EqFn> {
    source: Re<T>,
    eq: EqFn,
    sinks: BindSinks,
    state: RefCell<DedupByState<T>>,
}
struct DedupByState<T> {
    value: Option<T>,
    is_ready: bool,
    binds: Vec<Binding>,
}
impl<T: 'static, EqFn: Fn(&T, &T) -> bool + 'static> DedupBy<T, EqFn> {
    fn new(source: Re<T>, eq: EqFn) -> Self {
        Self {
            source,
            eq,
            sinks: BindSinks::new(),
            state: RefCell::new(DedupByState {
                value: None,
                is_ready: false,
                binds: Vec::new(),
            }),
        }
    }
    fn ready(self: &Rc<Self>) {
        let mut s = self.state.borrow_mut();
        let mut ctx = ReactiveContext::new(&self, &mut s.binds);
        let value = self.source.get(&mut ctx);
        if let Some(value_old) = &s.value {
            if (self.eq)(value_old, &value) {
                return;
            }
        }
        s.value = Some(value);
        drop(s);
        self.sinks.notify();
    }
}
impl<T: 'static, EqFn: Fn(&T, &T) -> bool + 'static> DynReBorrowSource for DedupBy<T, EqFn> {
    type Item = T;

    fn dyn_borrow(
        &self,
        rc_self: &Rc<dyn DynReBorrowSource<Item = Self::Item>>,
        ctx: &mut ReactiveContext,
    ) -> Ref<Self::Item> {
        let rc_self = Self::downcast(rc_self);
        let mut s = self.state.borrow();
        if s.is_ready {
            drop(s);
            rc_self.ready();
            s = self.state.borrow();
        }
        ctx.bind(rc_self);
        return Ref::map(s, |s| s.value.as_ref().unwrap());
    }

    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}
impl<T: 'static, EqFn: Fn(&T, &T) -> bool + 'static> BindSource for DedupBy<T, EqFn> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
impl<T: 'static, EqFn: Fn(&T, &T) -> bool + 'static> BindSink for DedupBy<T, EqFn> {
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
impl<T: 'static, EqFn: Fn(&T, &T) -> bool + 'static> Task for DedupBy<T, EqFn> {
    fn run(self: Rc<Self>) {
        self.ready();
    }
}

struct ForEach<T: 'static, F> {
    source: Re<T>,
    f: F,
    binds: RefCell<Vec<Binding>>,
}

impl<T: 'static, F: Fn(T) + 'static> ForEach<T, F> {
    fn new(source: Re<T>, f: F) -> Rc<Self> {
        let s = Rc::new(ForEach {
            source,
            f,
            binds: RefCell::new(Vec::new()),
        });
        s.next();
        s
    }

    fn next(self: &Rc<Self>) {
        let mut b = self.binds.borrow_mut();
        let mut ctx = ReactiveContext::new(&self, &mut b);
        (self.f)(self.source.get(&mut ctx));
    }
}
impl<T: 'static, F: Fn(T) + 'static> BindSink for ForEach<T, F> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        self.binds.borrow_mut().clear();
        ctx.spawn(Rc::downgrade(&self))
    }
}
impl<T: 'static, F: Fn(T) + 'static> Task for ForEach<T, F> {
    fn run(self: Rc<Self>) {
        self.next();
    }
}

struct ForEachBy<T, U, A, D>
where
    T: 'static,
    U: 'static,
    A: Fn(T) -> U + 'static,
    D: Fn(U) + 'static,
{
    source: Re<T>,
    attach: A,
    detach: D,
    value: RefCell<Option<U>>,
    binds: RefCell<Vec<Binding>>,
}

impl<T, U, A, D> ForEachBy<T, U, A, D>
where
    T: 'static,
    U: 'static,
    A: Fn(T) -> U + 'static,
    D: Fn(U) + 'static,
{
    fn new(source: Re<T>, attach: A, detach: D) -> Rc<Self> {
        let s = Rc::new(ForEachBy {
            source,
            attach,
            detach,
            value: RefCell::new(None),
            binds: RefCell::new(Vec::new()),
        });
        s.next();
        s
    }

    fn next(self: &Rc<Self>) {
        let mut b = self.binds.borrow_mut();
        let mut ctx = ReactiveContext::new(&self, &mut b);
        *self.value.borrow_mut() = Some((self.attach)(self.source.get(&mut ctx)));
    }
    fn detach_value(&self) {
        if let Some(value) = self.value.borrow_mut().take() {
            (self.detach)(value);
        }
    }
}
impl<T, U, A, D> BindSink for ForEachBy<T, U, A, D>
where
    T: 'static,
    U: 'static,
    A: Fn(T) -> U + 'static,
    D: Fn(U) + 'static,
{
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        self.binds.borrow_mut().clear();
        self.detach_value();
        ctx.spawn(Rc::downgrade(&self))
    }
}
impl<T, U, A, D> Task for ForEachBy<T, U, A, D>
where
    T: 'static,
    U: 'static,
    A: Fn(T) -> U + 'static,
    D: Fn(U) + 'static,
{
    fn run(self: Rc<Self>) {
        self.next();
    }
}
impl<T, U, A, D> Drop for ForEachBy<T, U, A, D>
where
    T: 'static,
    U: 'static,
    A: Fn(T) -> U + 'static,
    D: Fn(U) + 'static,
{
    fn drop(&mut self) {
        self.detach_value();
    }
}
