mod cell;
mod hot;
mod map_async;
mod ops;
mod scan;
mod tail;
mod to_stream;

pub use self::{cell::*, ops::*, tail::*};
use self::{hot::*, map_async::*, scan::*, to_stream::*};
use crate::bind::*;
use derivative::Derivative;
use futures::Future;
use std::{
    any::Any, borrow::Borrow, cell::Ref, cell::RefCell, iter::once, marker::PhantomData, rc::Rc,
    task::Poll,
};

trait DynRe: 'static {
    type Item;
    fn dyn_get(&self, ctx: &BindContext) -> Self::Item;

    fn to_re_ref(self: Rc<Self>) -> ReRef<Self::Item> {
        ReRef::new(self, |this, ctx, f| f(ctx, &this.dyn_get(ctx)))
    }
}

trait DynReSource: 'static {
    type Item;
    fn dyn_get(self: Rc<Self>, ctx: &BindContext) -> Self::Item;

    fn to_re_ref(self: Rc<Self>) -> ReRef<Self::Item> {
        ReRef::new(self, |this, ctx, f| f(ctx, &this.clone().dyn_get(ctx)))
    }
}

trait DynReBorrow: 'static {
    type Item: ?Sized;
    fn dyn_borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item>;
}
trait DynReBorrowSource: Any + 'static {
    type Item: ?Sized;

    fn dyn_borrow<'a>(
        &'a self,
        rc_self: &Rc<dyn DynReBorrowSource<Item = Self::Item>>,
        ctx: &BindContext<'a>,
    ) -> Ref<'a, Self::Item>;
    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any>;

    fn downcast(rc_self: &Rc<dyn DynReBorrowSource<Item = Self::Item>>) -> Rc<Self>
    where
        Self: Sized,
    {
        rc_self.clone().as_rc_any().downcast::<Self>().unwrap()
    }
}

trait DynReRef: 'static {
    type Item: ?Sized;
    fn dyn_with(&self, ctx: &BindContext, f: &mut dyn FnMut(&BindContext, &Self::Item));
}

#[must_use]
#[derive(Clone, Default)]
pub struct Subscription(Option<Rc<dyn Any>>);

impl Subscription {
    pub fn empty() -> Self {
        Subscription(None)
    }
}

pub trait LocalSpawn: 'static {
    type Handle;
    fn spawn_local(&self, fut: impl Future<Output = ()> + 'static) -> Self::Handle;
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub struct Re<T: 'static + ?Sized>(ReData<T>);

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
enum ReData<T: 'static + ?Sized> {
    Dyn(Rc<dyn DynRe<Item = T>>),
    DynSource(Rc<dyn DynReSource<Item = T>>),
}

impl<T: 'static> Re<T> {
    pub fn get(&self, ctx: &BindContext) -> T {
        match &self.0 {
            ReData::Dyn(rc) => rc.dyn_get(ctx),
            ReData::DynSource(rc) => rc.clone().dyn_get(ctx),
        }
    }
    pub fn head_tail(&self, scope: &BindContextScope) -> (T, Tail<T>) {
        Tail::new(self.clone(), scope)
    }

    pub fn new(get: impl Fn(&BindContext) -> T + 'static) -> Self {
        re(get).into_dyn()
    }
    pub fn constant(value: T) -> Self
    where
        T: Clone,
    {
        re_constant(value).into_dyn()
    }

    fn from_dyn(inner: impl DynRe<Item = T>) -> Self {
        Self(ReData::Dyn(Rc::new(inner)))
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
        let this = self.clone();
        ReBorrow::from_dyn_source(Scan::new((), move |_, ctx| this.get(ctx), |_| (), |x| x))
    }
    pub fn scan<St: 'static>(
        &self,
        initial_state: St,
        f: impl Fn(St, T) -> St + 'static,
    ) -> ReBorrow<St> {
        let this = self.clone();
        ReBorrow::from_dyn_source(Scan::new(
            initial_state,
            move |st, ctx| f(st, this.get(ctx)),
            |st| st,
            |st| st,
        ))
    }
    pub fn filter_scan<St: 'static>(
        &self,
        initial_state: St,
        predicate: impl Fn(&St, &T) -> bool + 'static,
        f: impl Fn(St, T) -> St + 'static,
    ) -> ReBorrow<St> {
        let this = self.clone();
        ReBorrow::from_dyn_source(FilterScan::new(
            initial_state,
            move |state, ctx| {
                let value = this.get(ctx);
                let is_notify = predicate(&state, &value);
                let state = if is_notify { f(state, value) } else { state };
                FilterScanResult { is_notify, state }
            },
            |state| state,
            |state| state,
        ))
    }

    pub fn dedup_by(&self, eq: impl Fn(&T, &T) -> bool + 'static) -> ReBorrow<T> {
        let this = self.clone();
        ReBorrow::from_dyn_source(FilterScan::new(
            None,
            move |state, ctx| {
                let mut value = this.get(ctx);
                let mut is_notify = false;
                if let Some(old) = state {
                    if eq(&value, &old) {
                        value = old;
                    } else {
                        is_notify = true;
                    }
                }
                FilterScanResult {
                    state: value,
                    is_notify,
                }
            },
            |value| Some(value),
            |value| value,
        ))
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

    pub fn fold<St: 'static>(
        &self,
        initial_state: St,
        f: impl Fn(St, T) -> St + 'static,
    ) -> Fold<St> {
        let this = self.clone();
        Fold::new(FoldBy::new(
            initial_state,
            move |st, ctx| (f(st, this.get(ctx)), ()),
            |(st, _)| st,
            |st| st,
        ))
    }
    pub fn collect_to<E: Extend<T> + 'static>(&self, e: E) -> Fold<E> {
        self.fold(e, |mut e, x| {
            e.extend(once(x));
            e
        })
    }
    pub fn collect<E: Extend<T> + Default + 'static>(&self) -> Fold<E> {
        self.collect_to(Default::default())
    }
    pub fn to_vec(&self) -> Fold<Vec<T>> {
        self.collect()
    }

    pub fn for_each(&self, f: impl FnMut(T) + 'static) -> Subscription {
        self.fold(f, move |mut f, x| {
            f(x);
            f
        })
        .into()
    }
    pub fn for_each_async_with<Fut>(
        &self,
        f: impl FnMut(T) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        let this = self.clone();
        let mut f = f;
        Fold::new(FoldBy::new(
            (),
            move |_, ctx| ((), sp.spawn_local(f(this.get(ctx)))),
            |_| (),
            |_| (),
        ))
        .into()
    }

    pub fn hot(&self) -> Self {
        Self(ReData::Dyn(Hot::new(self.clone())))
    }

    pub fn to_stream(&self) -> impl futures::Stream<Item = T> {
        ToStream::new(self.clone())
    }

    pub fn to_re_ref(&self) -> ReRef<T> {
        ReRef::new(self.clone(), |this, ctx, f| f(ctx, &this.get(ctx)))
    }
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

impl<T: 'static + ?Sized> ReBorrow<T> {
    pub fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, T> {
        match &self.0 {
            ReBorrowData::Dyn(rc) => rc.dyn_borrow(ctx),
            ReBorrowData::DynSource(rc) => rc.dyn_borrow(&rc, ctx),
        }
    }
    pub fn head_tail<'a>(&'a self, scope: &'a BindContextScope) -> (Ref<'a, T>, TailRef<T>) {
        TailRef::new_borrow(&self, scope)
    }

    pub fn constant(value: T) -> Self
    where
        T: Sized,
    {
        Self::new(RefCell::new(value), |cell, _ctx| cell.borrow())
    }
    pub fn new<S, F>(this: S, borrow: F) -> Self
    where
        S: 'static,
        for<'a> F: Fn(&'a S, &BindContext<'a>) -> Ref<'a, T> + 'static,
    {
        struct ReBorrowFn<S, F> {
            this: S,
            borrow: F,
        }
        impl<T, S, F> DynReBorrow for ReBorrowFn<S, F>
        where
            T: 'static + ?Sized,
            S: 'static,
            for<'a> F: Fn(&'a S, &BindContext<'a>) -> Ref<'a, T> + 'static,
        {
            type Item = T;
            fn dyn_borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, T> {
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
    pub fn map_ref<U: ?Sized>(&self, f: impl Fn(&T) -> &U + 'static) -> ReBorrow<U> {
        ReBorrow::new(self.clone(), move |this, ctx| {
            Ref::map(this.borrow(ctx), &f)
        })
    }
    pub fn map_borrow<B: ?Sized>(&self) -> ReBorrow<B>
    where
        T: Borrow<B>,
    {
        if let Some(b) = Any::downcast_ref::<ReBorrow<B>>(self) {
            b.clone()
        } else {
            self.map_ref(|x| x.borrow())
        }
    }

    pub fn flat_map<U>(&self, f: impl Fn(&T) -> Re<U> + 'static) -> Re<U> {
        self.map(f).flatten()
    }
    pub fn map_async_with<Fut>(
        &self,
        f: impl Fn(&T) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> ReBorrow<Poll<Fut::Output>>
    where
        Fut: Future + 'static,
    {
        self.to_re_ref().map_async_with(f, sp)
    }

    pub fn cloned(&self) -> Re<T>
    where
        T: Clone,
    {
        self.map(|x| x.clone())
    }

    pub fn scan<St: 'static>(
        &self,
        initial_state: St,
        f: impl Fn(St, &T) -> St + 'static,
    ) -> ReBorrow<St> {
        self.to_re_ref().scan(initial_state, f)
    }
    pub fn filter_scan<St: 'static>(
        &self,
        initial_state: St,
        predicate: impl Fn(&St, &T) -> bool + 'static,
        f: impl Fn(St, &T) -> St + 'static,
    ) -> ReBorrow<St> {
        self.to_re_ref().filter_scan(initial_state, predicate, f)
    }

    pub fn fold<St: 'static>(
        &self,
        initial_state: St,
        f: impl Fn(St, &T) -> St + 'static,
    ) -> Fold<St> {
        self.to_re_ref().fold(initial_state, f)
    }
    pub fn collect_to<E: for<'a> Extend<&'a T> + 'static>(&self, e: E) -> Fold<E> {
        self.fold(e, |mut e, x| {
            e.extend(once(x));
            e
        })
    }
    pub fn collect<E: for<'a> Extend<&'a T> + Default + 'static>(&self) -> Fold<E> {
        self.collect_to(Default::default())
    }
    pub fn to_vec(&self) -> Fold<Vec<T>>
    where
        T: Copy,
    {
        self.collect()
    }

    pub fn for_each(&self, f: impl FnMut(&T) + 'static) -> Subscription {
        self.to_re_ref().for_each(f)
    }
    pub fn for_each_async_with<Fut>(
        &self,
        f: impl FnMut(&T) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        self.to_re_ref().for_each_async_with(f, sp)
    }

    pub fn hot(&self) -> Self {
        let source = self.clone();
        Self(ReBorrowData::Dyn(Hot::new(source)))
    }

    pub fn to_re_ref(&self) -> ReRef<T> {
        ReRef::new(self.clone(), |this, ctx, f| f(ctx, &*this.borrow(ctx)))
    }
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub struct ReRef<T: 'static + ?Sized>(ReRefData<T>);

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
enum ReRefData<T: 'static + ?Sized> {
    StaticRef(&'static T),
    Dyn(Rc<dyn DynReRef<Item = T>>),
}

impl<T: 'static + ?Sized> ReRef<T> {
    pub fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &T) -> U) -> U {
        match &self.0 {
            ReRefData::StaticRef(x) => f(ctx, x),
            ReRefData::Dyn(source) => Self::dyn_with(&source, ctx, f),
        }
    }
    fn dyn_with<U>(
        this: &Rc<dyn DynReRef<Item = T>>,
        ctx: &BindContext,
        f: impl FnOnce(&BindContext, &T) -> U,
    ) -> U {
        let this = this.clone();
        let mut output = None;
        let mut f = Some(f);
        this.clone().dyn_with(ctx, &mut |ctx, value| {
            output = Some((f.take().unwrap())(ctx, value))
        });
        output.unwrap()
    }

    pub fn head_tail(&self, scope: &BindContextScope, f: impl FnOnce(&T)) -> TailRef<T> {
        TailRef::new(self.clone(), scope, f)
    }
    pub fn new<S: 'static>(
        this: S,
        f: impl Fn(&S, &BindContext, &mut dyn FnMut(&BindContext, &T)) + 'static,
    ) -> Self {
        struct ReRefFn<S, T: ?Sized, F> {
            this: S,
            f: F,
            _phantom: PhantomData<fn(&fn(&T))>,
        }
        impl<S, T, F> DynReRef for ReRefFn<S, T, F>
        where
            S: 'static,
            T: 'static + ?Sized,
            F: Fn(&S, &BindContext, &mut dyn FnMut(&BindContext, &T)) + 'static,
        {
            type Item = T;

            fn dyn_with(&self, ctx: &BindContext, f: &mut dyn FnMut(&BindContext, &T)) {
                (self.f)(&self.this, ctx, f)
            }
        }
        Self::from_dyn(ReRefFn {
            this,
            f,
            _phantom: PhantomData,
        })
    }
    pub fn constant(value: T) -> Self
    where
        T: Sized,
    {
        Self::new(value, |value, ctx, f| f(ctx, value))
    }
    pub fn static_ref(value: &'static T) -> Self {
        Self(ReRefData::StaticRef(value))
    }

    fn from_dyn(inner: impl DynReRef<Item = T>) -> Self {
        Self(ReRefData::Dyn(Rc::new(inner)))
    }

    pub fn map<U>(&self, f: impl Fn(&T) -> U + 'static) -> Re<U> {
        let this = self.clone();
        Re::new(move |ctx| this.with(ctx, |_ctx, x| f(x)))
    }
    pub fn map_ref<U: ?Sized>(&self, f: impl Fn(&T) -> &U + 'static) -> ReRef<U> {
        match &self.0 {
            ReRefData::StaticRef(x) => ReRef::static_ref(f(x)),
            ReRefData::Dyn(this) => ReRef::new(this.clone(), move |this, ctx, f_inner| {
                Self::dyn_with(this, ctx, |ctx, x| f_inner(ctx, f(x)))
            }),
        }
    }
    pub fn map_borrow<B: ?Sized>(&self) -> ReRef<B>
    where
        T: Borrow<B>,
    {
        if let Some(b) = Any::downcast_ref::<ReRef<B>>(self) {
            b.clone()
        } else {
            self.map_ref(|x| x.borrow())
        }
    }

    pub fn flat_map<U>(&self, f: impl Fn(&T) -> Re<U> + 'static) -> Re<U> {
        self.map(f).flatten()
    }
    pub fn map_async_with<Fut>(
        &self,
        f: impl Fn(&T) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> ReBorrow<Poll<Fut::Output>>
    where
        Fut: Future + 'static,
    {
        ReBorrow::from_dyn_source(MapAsync::new(self.map(f), sp))
    }
    pub fn scan<St: 'static>(
        &self,
        initial_state: St,
        f: impl Fn(St, &T) -> St + 'static,
    ) -> ReBorrow<St> {
        let this = self.clone();
        ReBorrow::from_dyn_source(Scan::new(
            initial_state,
            move |st, ctx| {
                let f = &f;
                this.with(ctx, move |_ctx, x| f(st, x))
            },
            |st| st,
            |st| st,
        ))
    }
    pub fn filter_scan<St: 'static>(
        &self,
        initial_state: St,
        predicate: impl Fn(&St, &T) -> bool + 'static,
        f: impl Fn(St, &T) -> St + 'static,
    ) -> ReBorrow<St> {
        let this = self.clone();
        ReBorrow::from_dyn_source(FilterScan::new(
            initial_state,
            move |state, ctx| {
                this.with(ctx, |_ctx, value| {
                    let is_notify = predicate(&state, &value);
                    let state = if is_notify { f(state, value) } else { state };
                    FilterScanResult { is_notify, state }
                })
            },
            |state| state,
            |state| state,
        ))
    }

    pub fn cloned(&self) -> Re<T>
    where
        T: Clone,
    {
        self.map(|x| x.clone())
    }
    pub fn fold<St: 'static>(
        &self,
        initial_state: St,
        f: impl Fn(St, &T) -> St + 'static,
    ) -> Fold<St> {
        let this = self.clone();
        let mut f = f;
        Fold::new(FoldBy::new(
            initial_state,
            move |st, ctx| {
                let f = &mut f;
                (this.with(ctx, move |_ctx, x| f(st, x)), ())
            },
            |(st, _)| st,
            |st| st,
        ))
    }
    pub fn collect_to<E: for<'a> Extend<&'a T> + 'static>(&self, e: E) -> Fold<E> {
        self.fold(e, |mut e, x| {
            e.extend(once(x));
            e
        })
    }
    pub fn collect<E: for<'a> Extend<&'a T> + Default + 'static>(&self) -> Fold<E> {
        self.collect_to(Default::default())
    }
    pub fn to_vec(&self) -> Fold<Vec<T>>
    where
        T: Copy,
    {
        self.collect()
    }
    pub fn for_each(&self, f: impl FnMut(&T) + 'static) -> Subscription {
        self.fold(f, move |mut f, x| {
            f(x);
            f
        })
        .into()
    }
    pub fn for_each_async_with<Fut>(
        &self,
        f: impl FnMut(&T) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        let this = self.clone();
        let mut f = f;
        Fold::new(FoldBy::new(
            (),
            move |_, ctx| ((), this.with(ctx, |_ctx, x| sp.spawn_local(f(x)))),
            |_| (),
            |_| (),
        ))
        .into()
    }

    pub fn hot(&self) -> Self {
        let source = self.clone();
        Self(ReRefData::Dyn(Hot::new(source)))
    }
}
impl<T: 'static> Re<Re<T>> {
    pub fn flatten(&self) -> Re<T> {
        let this = self.clone();
        Re::new(move |ctx| this.get(ctx).get(ctx))
    }
}
impl<T: 'static> ReBorrow<Re<T>> {
    pub fn flatten(&self) -> Re<T> {
        let this = self.clone();
        Re::new(move |ctx| this.borrow(ctx).get(ctx))
    }
}

impl<T: 'static> ReRef<Re<T>> {
    pub fn flatten(&self) -> Re<T> {
        let this = self.clone();
        Re::new(move |ctx| this.with(ctx, |ctx, x| x.get(ctx)))
    }
}

trait DynFold {
    type Output;

    fn stop(&self) -> Self::Output;
    fn as_dyn_any(self: Rc<Self>) -> Rc<dyn Any>;
}
pub struct Fold<T>(FoldData<T>);

enum FoldData<T> {
    Constant(T),
    Dyn(Rc<dyn DynFold<Output = T>>),
}

impl<T> Fold<T> {
    fn new(fold: Rc<dyn DynFold<Output = T>>) -> Self {
        Self(FoldData::Dyn(fold))
    }
    fn constant(value: T) -> Self {
        Self(FoldData::Constant(value))
    }

    pub fn stop(self) -> T {
        match self.0 {
            FoldData::Constant(value) => value,
            FoldData::Dyn(this) => this.stop(),
        }
    }
}
impl<T> From<Fold<T>> for Subscription {
    fn from(x: Fold<T>) -> Self {
        match x.0 {
            FoldData::Constant(_) => Subscription::empty(),
            FoldData::Dyn(this) => Subscription(Some(this.as_dyn_any())),
        }
    }
}

#[derive(Clone)]
pub enum MayRe<T: 'static> {
    Value(T),
    Re(Re<T>),
}

pub struct MayReRef<T: ?Sized + 'static>(ReRef<T>);

impl<T> Clone for MayReRef<T>
where
    T: Clone + ?Sized + 'static,
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: 'static> From<T> for MayRe<T> {
    fn from(value: T) -> Self {
        MayRe::Value(value)
    }
}
impl<T: Copy + 'static> From<&T> for MayRe<T> {
    fn from(value: &T) -> Self {
        MayRe::Value(*value)
    }
}
impl<T: 'static> From<Re<T>> for MayRe<T> {
    fn from(source: Re<T>) -> Self {
        MayRe::Re(source)
    }
}
impl<T: 'static> From<&Re<T>> for MayRe<T> {
    fn from(source: &Re<T>) -> Self {
        MayRe::Re(source.clone())
    }
}
impl<T: Copy + 'static> From<ReRef<T>> for MayRe<T> {
    fn from(source: ReRef<T>) -> Self {
        MayRe::Re(source.cloned())
    }
}
impl<T: Copy + 'static> From<&ReRef<T>> for MayRe<T> {
    fn from(source: &ReRef<T>) -> Self {
        MayRe::Re(source.cloned())
    }
}
impl<T: Copy + 'static> From<ReBorrow<T>> for MayRe<T> {
    fn from(source: ReBorrow<T>) -> Self {
        MayRe::Re(source.cloned())
    }
}
impl<T: Copy + 'static> From<&ReBorrow<T>> for MayRe<T> {
    fn from(source: &ReBorrow<T>) -> Self {
        MayRe::Re(source.cloned())
    }
}

impl<T> From<&'static T> for MayReRef<T>
where
    T: ?Sized + 'static,
{
    fn from(r: &'static T) -> Self {
        Self(ReRef::static_ref(r))
    }
}

impl<T, B> From<Re<B>> for MayReRef<T>
where
    T: ?Sized + 'static,
    B: Borrow<T>,
{
    fn from(source: Re<B>) -> Self {
        source.to_re_ref().into()
    }
}
impl<T> From<&Re<T>> for MayReRef<T>
where
    T: 'static,
{
    fn from(source: &Re<T>) -> Self {
        source.to_re_ref().into()
    }
}

impl<T, B> From<ReRef<B>> for MayReRef<T>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn from(source: ReRef<B>) -> Self {
        Self(source.map_borrow())
    }
}
impl<T> From<&ReRef<T>> for MayReRef<T>
where
    T: ?Sized + 'static,
{
    fn from(source: &ReRef<T>) -> Self {
        Self(source.map_borrow())
    }
}
impl<T, B> From<ReBorrow<B>> for MayReRef<T>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn from(source: ReBorrow<B>) -> Self {
        source.to_re_ref().into()
    }
}
impl<T> From<&ReBorrow<T>> for MayReRef<T>
where
    T: ?Sized + 'static,
{
    fn from(source: &ReBorrow<T>) -> Self {
        source.to_re_ref().into()
    }
}

impl From<String> for MayReRef<str> {
    fn from(value: String) -> Self {
        if value.is_empty() {
            "".into()
        } else {
            ReRef::constant(value).into()
        }
    }
}
impl From<&Re<String>> for MayReRef<str> {
    fn from(value: &Re<String>) -> Self {
        value.to_re_ref().into()
    }
}
impl From<&ReRef<String>> for MayReRef<str> {
    fn from(value: &ReRef<String>) -> Self {
        value.map_ref(|s| s.as_str()).into()
    }
}
impl From<&ReBorrow<String>> for MayReRef<str> {
    fn from(value: &ReBorrow<String>) -> Self {
        value.to_re_ref().into()
    }
}
