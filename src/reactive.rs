mod cell;
mod hot;
mod into_stream;
mod map_async;
mod may_re;
mod re;
mod re_borrow;
mod re_borrow_ops;
mod re_ops;
mod re_ref;
mod re_ref_ops;
mod scan;
mod scan2;
mod tail;

pub use self::{
    cell::*, may_re::*, re::*, re_borrow::*, re_borrow_ops::*, re_ops::*, re_ref::*, re_ref_ops::*,
    tail::*,
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
        ReBorrow::from_dyn(Rc::new(IntoDyn(self)))
    }
}

pub trait ReactiveRef: 'static {
    type Item: ?Sized;
    fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &Self::Item) -> U) -> U;

    fn into_re_ref(self) -> ReRef<Self::Item>
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
        ReRef::from_dyn(Rc::new(IntoDyn(self)))
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

trait DynamicFold {
    type Output;

    fn stop(self: Rc<Self>, scope: &BindContextScope) -> Self::Output;
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
        BindContextScope::with(move |scope| self.stop_with(scope))
    }

    pub fn stop_with(self, scope: &BindContextScope) -> T {
        match self.0 {
            FoldData::Constant(value) => value,
            FoldData::Dyn(this) => this.stop(scope),
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
