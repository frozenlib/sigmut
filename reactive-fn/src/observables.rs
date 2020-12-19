pub mod cell;
pub mod collector;
mod dyn_obs;
mod dyn_obs_borrow;
mod dyn_obs_ref;
mod hot;
mod into_stream;
mod map_async;
mod obs;
mod obs_borrow;
mod obs_ref;
mod scan;
mod tail;
mod value_obs;

pub use self::{
    cell::{ObsCell, ObsRefCell},
    collector::{Collect, ObsAnyCollector, ObsCollector, ObsSomeCollector},
    dyn_obs::*,
    dyn_obs_borrow::*,
    dyn_obs_ref::*,
    obs::*,
    obs_borrow::*,
    obs_ref::*,
    tail::*,
    value_obs::*,
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

pub trait Observable: 'static {
    type Item;
    fn get(&self, cx: &BindContext) -> Self::Item;

    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        struct IntoDyn<S>(S);
        impl<S: Observable> DynamicObservable for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_get(&self, cx: &BindContext) -> Self::Item {
                self.0.get(cx)
            }
            fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = Self::Item>> {
                self
            }
        }
        impl<S: Observable> DynamicObservableRef for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
                f(&self.0.get(cx), cx)
            }
        }
        DynObs::from_dyn(IntoDyn(self))
    }
    fn into_obs(self) -> Obs<Self>
    where
        Self: Sized,
    {
        Obs(self)
    }
}

pub trait ObservableBorrow: 'static {
    type Item: ?Sized;
    fn borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item>;

    fn into_dyn(self) -> DynObsBorrow<Self::Item>
    where
        Self: Sized,
    {
        struct IntoDyn<S>(S);
        impl<S: ObservableBorrow> DynamicObservableBorrow for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item> {
                self.0.borrow(cx)
            }
            fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = Self::Item>> {
                self
            }
        }
        impl<S: ObservableBorrow> DynamicObservableRef for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
                f(&self.0.borrow(cx), cx)
            }
        }
        DynObsBorrow::from_dyn(Rc::new(IntoDyn(self)))
    }
}

pub trait ObservableRef: 'static {
    type Item: ?Sized;
    fn with<U>(&self, f: impl FnOnce(&Self::Item, &BindContext) -> U, cx: &BindContext) -> U;

    fn into_dyn(self) -> DynObsRef<Self::Item>
    where
        Self: Sized,
    {
        struct IntoDyn<S>(S);
        impl<S: ObservableRef> DynamicObservableRef for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
                self.0.with(f, cx)
            }
        }
        DynObsRef::from_dyn(Rc::new(IntoDyn(self)))
    }
}
pub(crate) trait DynamicObservable: 'static {
    type Item;
    fn dyn_get(&self, cx: &BindContext) -> Self::Item;
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = Self::Item>>;
}

pub(crate) trait DynamicObservableSource: 'static {
    type Item;
    fn dyn_get(self: Rc<Self>, cx: &BindContext) -> Self::Item;
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRefSource<Item = Self::Item>>;
}

pub(crate) trait DynamicObservableBorrow: 'static {
    type Item: ?Sized;
    fn dyn_borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item>;
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = Self::Item>>;
}
pub(crate) trait DynamicObservableBorrowSource: Any + 'static {
    type Item: ?Sized;

    fn dyn_borrow<'a>(
        &'a self,
        rc_self: &Rc<dyn DynamicObservableBorrowSource<Item = Self::Item>>,
        cx: &BindContext<'a>,
    ) -> Ref<'a, Self::Item>;
    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any>;

    fn downcast(rc_self: &Rc<dyn DynamicObservableBorrowSource<Item = Self::Item>>) -> Rc<Self>
    where
        Self: Sized,
    {
        rc_self.clone().as_rc_any().downcast::<Self>().unwrap()
    }

    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRefSource<Item = Self::Item>>;
}

pub(crate) trait DynamicObservableRef: 'static {
    type Item: ?Sized;
    fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext);
}
pub(crate) trait DynamicObservableRefSource: 'static {
    type Item: ?Sized;
    fn dyn_with(self: Rc<Self>, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext);
}

pub trait Observer<T>: 'static {
    fn next(&mut self, value: T);
}
impl<F: FnMut(T) + 'static, T> Observer<T> for F {
    fn next(&mut self, value: T) {
        self(value)
    }
}

pub trait CollectObserver<T> {
    type Observer: Observer<T>;
    fn insert(&self) -> Self::Observer;

    fn insert_obs(&self, obs: impl Observable<Item = T>) -> Subscription {
        obs.into_obs().subscribe(self.insert())
    }
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
