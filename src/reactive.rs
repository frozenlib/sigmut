mod cell;
mod dyn_re;
mod dyn_re_borrow;
mod dyn_re_ref;
mod hot;
mod into_stream;
mod map_async;
mod re_borrow_ops;
mod re_ops;
mod re_ref_ops;
mod scan;
mod tail;

pub use self::{
    cell::*, dyn_re::*, dyn_re_borrow::*, dyn_re_ref::*, re_borrow_ops::*, re_ops::*,
    re_ref_ops::*, tail::*,
};
use self::{hot::*, into_stream::*, map_async::*, scan::*};
use crate::bind::*;
use derivative::Derivative;
use futures::Future;
use std::{
    any::Any, borrow::Borrow, cell::Ref, cell::RefCell, iter::once, marker::PhantomData, rc::Rc,
    task::Poll,
};

pub trait Reactive: 'static {
    type Item;
    fn get(&self, ctx: &BindContext) -> Self::Item;

    fn into_re(self) -> Re<Self::Item>
    where
        Self: Sized,
    {
        struct IntoDyn<S>(S);
        impl<S: Reactive> DynamicReactive for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_get(&self, ctx: &BindContext) -> Self::Item {
                self.0.get(ctx)
            }
            fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicReactiveRef<Item = Self::Item>> {
                self
            }
        }
        impl<S: Reactive> DynamicReactiveRef for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_with(&self, ctx: &BindContext, f: &mut dyn FnMut(&BindContext, &Self::Item)) {
                f(ctx, &self.0.get(ctx))
            }
        }
        Re::from_dyn(IntoDyn(self))
    }
}

pub trait ReactiveBorrow: 'static {
    type Item: ?Sized;
    fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item>;

    fn into_re_borrow(self) -> ReBorrow<Self::Item>
    where
        Self: Sized,
    {
        struct IntoDyn<S>(S);
        impl<S: ReactiveBorrow> DynamicReactiveBorrow for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item> {
                self.0.borrow(ctx)
            }
            fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicReactiveRef<Item = Self::Item>> {
                self
            }
        }
        impl<S: ReactiveBorrow> DynamicReactiveRef for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_with(&self, ctx: &BindContext, f: &mut dyn FnMut(&BindContext, &Self::Item)) {
                f(ctx, &self.0.borrow(ctx))
            }
        }
        ReBorrow::from_dyn(IntoDyn(self))
    }
}

pub trait ReactiveRef: 'static {
    type Item: ?Sized;
    fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &Self::Item) -> U) -> U;

    fn into_dyn(self) -> ReRef<Self::Item>
    where
        Self: Sized,
    {
        struct IntoDyn<S>(S);
        impl<S: ReactiveRef> DynamicReactiveRef for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_with(&self, ctx: &BindContext, f: &mut dyn FnMut(&BindContext, &Self::Item)) {
                self.0.with(ctx, f)
            }
        }
        ReRef::from_dyn(IntoDyn(self))
    }
}
trait DynamicReactive: 'static {
    type Item;
    fn dyn_get(&self, ctx: &BindContext) -> Self::Item;
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicReactiveRef<Item = Self::Item>>;
}

trait DynamicReactiveSource: 'static {
    type Item;
    fn dyn_get(self: Rc<Self>, ctx: &BindContext) -> Self::Item;
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicReactiveRefSource<Item = Self::Item>>;
}

trait DynamicReactiveBorrow: 'static {
    type Item: ?Sized;
    fn dyn_borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item>;
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicReactiveRef<Item = Self::Item>>;
}
trait DynamicReactiveBorrowSource: Any + 'static {
    type Item: ?Sized;

    fn dyn_borrow<'a>(
        &'a self,
        rc_self: &Rc<dyn DynamicReactiveBorrowSource<Item = Self::Item>>,
        ctx: &BindContext<'a>,
    ) -> Ref<'a, Self::Item>;
    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any>;

    fn downcast(rc_self: &Rc<dyn DynamicReactiveBorrowSource<Item = Self::Item>>) -> Rc<Self>
    where
        Self: Sized,
    {
        rc_self.clone().as_rc_any().downcast::<Self>().unwrap()
    }

    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicReactiveRefSource<Item = Self::Item>>;
}

trait DynamicReactiveRef: 'static {
    type Item: ?Sized;
    fn dyn_with(&self, ctx: &BindContext, f: &mut dyn FnMut(&BindContext, &Self::Item));
}
trait DynamicReactiveRefSource: 'static {
    type Item: ?Sized;
    fn dyn_with(self: Rc<Self>, ctx: &BindContext, f: &mut dyn FnMut(&BindContext, &Self::Item));
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

trait DynamicFold {
    type Output;

    fn stop(&self) -> Self::Output;
    fn as_dyn_any(self: Rc<Self>) -> Rc<dyn Any>;
}
pub struct Fold<T>(FoldData<T>);

enum FoldData<T> {
    Constant(T),
    Dyn(Rc<dyn DynamicFold<Output = T>>),
}

impl<T> Fold<T> {
    fn new(fold: Rc<dyn DynamicFold<Output = T>>) -> Self {
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
        source.as_ref().into()
    }
}
impl<T> From<&Re<T>> for MayReRef<T>
where
    T: 'static,
{
    fn from(source: &Re<T>) -> Self {
        source.as_ref().into()
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
        source.as_ref().into()
    }
}
impl<T> From<&ReBorrow<T>> for MayReRef<T>
where
    T: ?Sized + 'static,
{
    fn from(source: &ReBorrow<T>) -> Self {
        source.as_ref().into()
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
        value.as_ref().into()
    }
}
impl From<&ReRef<String>> for MayReRef<str> {
    fn from(value: &ReRef<String>) -> Self {
        value.map_ref(|s| s.as_str()).into()
    }
}
impl From<&ReBorrow<String>> for MayReRef<str> {
    fn from(value: &ReBorrow<String>) -> Self {
        value.as_ref().into()
    }
}
