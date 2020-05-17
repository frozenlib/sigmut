mod cached;
mod cell;
mod for_each;
mod map_async;

pub use self::cell::*;
use self::{cached::*, for_each::*, map_async::*};
use crate::bind::*;
use futures::Future;
use std::{
    any::Any,
    borrow::Cow,
    cell::Ref,
    cell::RefCell,
    marker::PhantomData,
    rc::{Rc, Weak},
    task::Poll,
};

trait DynRe: 'static {
    type Item;
    fn dyn_get(&self, ctx: &mut BindContext) -> Self::Item;
}

trait DynReSource: 'static {
    type Item;
    fn dyn_get(self: Rc<Self>, ctx: &mut BindContext) -> Self::Item;
}

trait DynReBorrow: 'static {
    type Item: ?Sized;
    fn dyn_borrow(&self, ctx: &mut BindContext) -> Ref<Self::Item>;
}
trait DynReBorrowSource: Any + 'static {
    type Item;

    fn dyn_borrow(
        &self,
        rc_self: &Rc<dyn DynReBorrowSource<Item = Self::Item>>,
        ctx: &mut BindContext,
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
    fn dyn_with(&self, ctx: &mut BindContext, f: &mut dyn FnMut(&Self::Item));
}

pub struct Unbind(Rc<dyn Any>);
pub trait LocalSpawn: 'static {
    type Handle;
    fn spawn_local<Fut: Future<Output = ()>>(&self, fut: Fut) -> Self::Handle;
}

use derivative::Derivative;

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub struct Re<T: 'static + ?Sized>(ReData<T>);

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
enum ReData<T: 'static + ?Sized> {
    Dyn(Rc<dyn DynRe<Item = T>>),
    DynSource(Rc<dyn DynReSource<Item = T>>),
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub struct ReBorrow<T: 'static + ?Sized>(ReBorrowData<T>);

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
enum ReBorrowData<T: 'static + ?Sized> {
    Dyn(Rc<dyn DynReBorrow<Item = T>>),
    DynSource(Rc<dyn DynReBorrowSource<Item = T>>),
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub struct ReRef<T: 'static + ?Sized>(Rc<dyn DynReRef<Item = T>>);

impl<T: 'static> Re<T> {
    pub fn get(&self, ctx: &mut BindContext) -> T {
        match &self.0 {
            ReData::Dyn(rc) => rc.dyn_get(ctx),
            ReData::DynSource(rc) => rc.clone().dyn_get(ctx),
        }
    }

    pub fn new(get: impl Fn(&mut BindContext) -> T + 'static) -> Self {
        struct ReFn<F>(F);
        impl<F: Fn(&mut BindContext) -> T + 'static, T> DynRe for ReFn<F> {
            type Item = T;
            fn dyn_get(&self, ctx: &mut BindContext) -> Self::Item {
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

    pub fn map<U>(&self, f: impl Fn(T) -> U + 'static) -> Re<U> {
        let this = self.clone();
        Re::new(move |ctx| f(this.get(ctx)))
    }
    pub fn flat_map<U>(&self, f: impl Fn(T) -> Re<U> + 'static) -> Re<U> {
        self.map(f).flatten()
    }
    pub fn map_async_with<Fut>(
        &self,
        f: impl Fn(T) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> ReBorrow<Poll<Fut::Output>>
    where
        Fut: Future + 'static,
    {
        ReBorrow::from_dyn_source(MapAsync::new(self.map(f), sp))
    }

    pub fn cached(&self) -> ReBorrow<T> {
        ReBorrow::from_dyn_source(Cached::new(self.clone()))
    }

    pub fn dedup_by(&self, eq: impl Fn(&T, &T) -> bool + 'static) -> ReBorrow<T> {
        ReBorrow::from_dyn_source(DedupBy::new(self.clone(), eq))
    }
    pub fn dedup_by_key<K: PartialEq>(&self, to_key: impl Fn(&T) -> K + 'static) -> ReBorrow<T> {
        self.dedup_by(move |l, r| to_key(l) == to_key(r))
    }

    pub fn dedup(&self) -> ReBorrow<T>
    where
        T: PartialEq,
    {
        self.dedup_by(|l, r| l == r)
    }

    pub fn for_each(&self, f: impl FnMut(T) + 'static) -> Unbind {
        Unbind(ForEach::new(self.clone(), f))
    }
    pub fn for_each_by<U: 'static>(
        &self,
        attach: impl FnMut(T) -> U + 'static,
        detach: impl FnMut(U) + 'static,
    ) -> Unbind {
        Unbind(ForEachBy::new(self.clone(), attach, detach))
    }
    pub fn for_each_async_with<Fut>(
        &self,
        f: impl FnMut(T) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> Unbind
    where
        Fut: Future<Output = ()> + 'static,
    {
        let mut f = f;
        self.for_each_by(move |value| sp.spawn_local(f(value)), move |_| {})
    }

    pub fn to_re_ref(&self) -> ReRef<T> {
        ReRef::new(self.clone(), |this, ctx, f| f(&this.get(ctx)))
    }
}
impl<T: 'static> Re<Re<T>> {
    pub fn flatten(&self) -> Re<T> {
        let this = self.clone();
        Re::new(move |ctx| this.get(ctx).get(ctx))
    }
}

impl<T: 'static> ReBorrow<T> {
    pub fn borrow(&self, ctx: &mut BindContext) -> Ref<T> {
        match &self.0 {
            ReBorrowData::Dyn(rc) => rc.dyn_borrow(ctx),
            ReBorrowData::DynSource(rc) => rc.dyn_borrow(&rc, ctx),
        }
    }

    pub fn constant(value: T) -> Self {
        Self::new(RefCell::new(value), |cell, _ctx| cell.borrow())
    }
    pub fn new<S, F>(this: S, borrow: F) -> Self
    where
        S: 'static,
        for<'a> F: Fn(&'a S, &mut BindContext) -> Ref<'a, T> + 'static,
    {
        struct ReBorrowFn<S, F> {
            this: S,
            borrow: F,
        }
        impl<T, S, F> DynReBorrow for ReBorrowFn<S, F>
        where
            T: 'static,
            S: 'static,
            for<'a> F: Fn(&'a S, &mut BindContext) -> Ref<'a, T> + 'static,
        {
            type Item = T;
            fn dyn_borrow(&self, ctx: &mut BindContext) -> Ref<T> {
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

    pub fn map<U>(&self, f: impl Fn(&T) -> U + 'static) -> Re<U> {
        let this = self.clone();
        Re::new(move |ctx| f(&this.borrow(ctx)))
    }

    pub fn cloned(&self) -> Re<T>
    where
        T: Clone,
    {
        self.map(|x| x.clone())
    }

    pub fn for_each(&self, f: impl FnMut(&T) + 'static) -> Unbind {
        self.to_re_ref().for_each(f)
    }

    pub fn to_re_ref(&self) -> ReRef<T> {
        ReRef::new(self.clone(), |this, ctx, f| f(&*this.borrow(ctx)))
    }
}
impl<T: 'static> ReRef<T> {
    pub fn with<U>(&self, ctx: &mut BindContext, mut f: impl FnMut(&T) -> U) -> U {
        let mut output = None;
        self.0.dyn_with(ctx, &mut |value| output = Some(f(value)));
        output.unwrap()
    }
    pub fn new<S: 'static>(
        this: S,
        f: impl Fn(&S, &mut BindContext, &mut dyn FnMut(&T)) + 'static,
    ) -> Self {
        struct ReRefFn<S, T, F> {
            this: S,
            f: F,
            _phantom: PhantomData<fn(&fn(&T))>,
        }
        impl<S, T, F> DynReRef for ReRefFn<S, T, F>
        where
            S: 'static,
            T: 'static,
            F: Fn(&S, &mut BindContext, &mut dyn FnMut(&T)) + 'static,
        {
            type Item = T;

            fn dyn_with(&self, ctx: &mut BindContext, f: &mut dyn FnMut(&T)) {
                (self.f)(&self.this, ctx, f)
            }
        }
        Self::from_dyn(ReRefFn {
            this,
            f,
            _phantom: PhantomData,
        })
    }
    fn from_dyn(inner: impl DynReRef<Item = T>) -> Self {
        Self(Rc::new(inner))
    }

    pub fn map<U>(&self, f: impl Fn(&T) -> U + 'static) -> Re<U> {
        let this = self.clone();
        Re::new(move |ctx| this.with(ctx, |x| f(x)))
    }
    pub fn for_each(&self, f: impl FnMut(&T) + 'static) -> Unbind {
        Unbind(Rc::new(ForEachRef::new(self.clone(), f)))
    }
}

pub enum ReCow<T: 'static + ToOwned + ?Sized> {
    Cow(Cow<'static, T>),
    ReRef(ReRef<T>),
    ReRefOwned(ReRef<T::Owned>),
}

pub trait IntoReCow<T: 'static + ToOwned + ?Sized> {
    fn into_re_cow(self) -> ReCow<T>;
}

impl<T: 'static + Clone> IntoReCow<T> for &T {
    fn into_re_cow(self) -> ReCow<T> {
        ReCow::Cow(Cow::Owned(self.clone()))
    }
}

impl<T: 'static + Clone> IntoReCow<T> for T {
    fn into_re_cow(self) -> ReCow<T> {
        ReCow::Cow(Cow::Owned(self))
    }
}
impl<T: 'static + Clone> IntoReCow<T> for ReRef<T> {
    fn into_re_cow(self) -> ReCow<T> {
        ReCow::ReRef(self)
    }
}
impl<T: 'static + Clone> IntoReCow<T> for Re<T> {
    fn into_re_cow(self) -> ReCow<T> {
        self.to_re_ref().into_re_cow()
    }
}
impl<T: 'static + Clone> IntoReCow<T> for ReBorrow<T> {
    fn into_re_cow(self) -> ReCow<T> {
        self.to_re_ref().into_re_cow()
    }
}

pub struct ReStr(ReCow<str>);

trait IntoReStr {
    fn into_re_str(self) -> ReStr;
}
impl IntoReStr for Cow<'static, str> {
    fn into_re_str(self) -> ReStr {
        ReStr(ReCow::Cow(self))
    }
}
impl IntoReStr for &'static str {
    fn into_re_str(self) -> ReStr {
        ReStr(ReCow::Cow(Cow::Borrowed(self)))
    }
}
impl IntoReStr for &'static String {
    fn into_re_str(self) -> ReStr {
        ReStr(ReCow::Cow(Cow::Borrowed(&self)))
    }
}
impl IntoReStr for String {
    fn into_re_str(self) -> ReStr {
        ReStr(ReCow::Cow(Cow::Owned(self)))
    }
}
impl IntoReStr for ReRef<str> {
    fn into_re_str(self) -> ReStr {
        ReStr(ReCow::ReRef(self))
    }
}
impl IntoReStr for ReRef<String> {
    fn into_re_str(self) -> ReStr {
        ReStr(ReCow::ReRefOwned(self))
    }
}
impl IntoReStr for ReBorrow<str> {
    fn into_re_str(self) -> ReStr {
        todo!()
    }
}
impl IntoReStr for ReBorrow<String> {
    fn into_re_str(self) -> ReStr {
        ReStr(ReCow::ReRefOwned(self.to_re_ref()))
    }
}

struct DedupBy<T: 'static, EqFn> {
    source: Re<T>,
    eq: EqFn,
    sinks: BindSinks,
    state: RefCell<DedupByState<T>>,
}
struct DedupByState<T> {
    value: Option<T>,
    is_ready: bool,
    bindings: Vec<Binding>,
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
                bindings: Vec::new(),
            }),
        }
    }
    fn ready(self: &Rc<Self>) {
        let mut s = self.state.borrow_mut();
        let mut ctx = BindContext::new(&self, &mut s.bindings);
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
        ctx: &mut BindContext,
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
            s.bindings.clear();
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

struct Cache<S> {
    sinks: BindSinks,
    data: RefCell<CacheData<S>>,
}
struct CacheData<S> {
    bindings: Vec<Binding>,
    is_ready: bool,
    state: S,
}
trait CacheState: Sized {
    type Item;

    fn notify(&mut self) -> bool;
    fn ready(&mut self, rs_self: Rc<Cache<Self>>);
    fn value(&self) -> &Self::Item;
}

impl<S> Cache<S> {
    pub fn new(state: S) -> Rc<Self> {
        let rc = Rc::new(Cache {
            sinks: BindSinks::new(),
            data: RefCell::new(CacheData {
                bindings: Vec::new(),
                is_ready: false,
                state,
            }),
        });
        rc
    }
}
impl<S: 'static> BindSource for Cache<S> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
