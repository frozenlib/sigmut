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
mod tail;

pub use self::{
    cell::*, may_re::*, re::*, re_borrow::*, re_borrow_ops::*, re_ops::*, re_ref::*, re_ref_ops::*,
    tail::*,
};
use self::{hot::*, into_stream::*, map_async::*, scan::*};
use crate::{bind::*, BindScope, NotifyScope};
use derivative::Derivative;
use std::{
    any::Any,
    borrow::Borrow,
    cell::{Ref, RefCell},
    future::Future,
    iter::once,
    marker::PhantomData,
    rc::Rc,
    task::Poll,
};

pub trait Reactive: 'static {
    type Item;
    fn get(&self, cx: &BindContext) -> Self::Item;

    fn into_re(self) -> Re<Self::Item>
    where
        Self: Sized,
    {
        struct IntoDyn<S>(S);
        impl<S: Reactive> DynamicReactive for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_get(&self, cx: &BindContext) -> Self::Item {
                self.0.get(cx)
            }
            fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicReactiveRef<Item = Self::Item>> {
                self
            }
        }
        impl<S: Reactive> DynamicReactiveRef for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
                f(&self.0.get(cx), cx)
            }
        }
        Re::from_dyn(IntoDyn(self))
    }
}

pub trait ReactiveBorrow: 'static {
    type Item: ?Sized;
    fn borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item>;

    fn into_re_borrow(self) -> ReBorrow<Self::Item>
    where
        Self: Sized,
    {
        struct IntoDyn<S>(S);
        impl<S: ReactiveBorrow> DynamicReactiveBorrow for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item> {
                self.0.borrow(cx)
            }
            fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicReactiveRef<Item = Self::Item>> {
                self
            }
        }
        impl<S: ReactiveBorrow> DynamicReactiveRef for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
                f(&self.0.borrow(cx), cx)
            }
        }
        ReBorrow::from_dyn(Rc::new(IntoDyn(self)))
    }
}

pub trait ReactiveRef: 'static {
    type Item: ?Sized;
    fn with<U>(&self, f: impl FnOnce(&Self::Item, &BindContext) -> U, cx: &BindContext) -> U;

    fn into_re_ref(self) -> ReRef<Self::Item>
    where
        Self: Sized,
    {
        struct IntoDyn<S>(S);
        impl<S: ReactiveRef> DynamicReactiveRef for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
                self.0.with(f, cx)
            }
        }
        ReRef::from_dyn(Rc::new(IntoDyn(self)))
    }
}
trait DynamicReactive: 'static {
    type Item;
    fn dyn_get(&self, cx: &BindContext) -> Self::Item;
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicReactiveRef<Item = Self::Item>>;
}

trait DynamicReactiveSource: 'static {
    type Item;
    fn dyn_get(self: Rc<Self>, cx: &BindContext) -> Self::Item;
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicReactiveRefSource<Item = Self::Item>>;
}

trait DynamicReactiveBorrow: 'static {
    type Item: ?Sized;
    fn dyn_borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item>;
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicReactiveRef<Item = Self::Item>>;
}
trait DynamicReactiveBorrowSource: Any + 'static {
    type Item: ?Sized;

    fn dyn_borrow<'a>(
        &'a self,
        rc_self: &Rc<dyn DynamicReactiveBorrowSource<Item = Self::Item>>,
        cx: &BindContext<'a>,
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
    fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext);
}
trait DynamicReactiveRefSource: 'static {
    type Item: ?Sized;
    fn dyn_with(self: Rc<Self>, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext);
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

    fn stop(self: Rc<Self>, scope: &BindScope) -> Self::Output;
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
        BindScope::with(move |scope| self.stop_with(scope))
    }

    pub fn stop_with(self, scope: &BindScope) -> T {
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

pub fn spawn(mut f: impl FnMut(&BindContext) + 'static) -> Subscription {
    Subscription(Some(FoldBy::new(
        (),
        fold_by_op(
            move |st, cx| {
                f(cx);
                st
            },
            |st| st,
            |st| st,
        ),
    )))
}
