pub mod cell;
pub mod collector;
mod dyn_obs;
mod dyn_obs_borrow;
mod dyn_obs_ref;
mod dynamic_obs;
mod hot;
mod into_stream;
mod map_async;
mod obs;
mod obs_borrow;
mod obs_ref;
mod scan;
mod tail;
mod value_obs;

pub(crate) use self::dynamic_obs::*;
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
        DynObs::from_dyn(DynamicObs(Obs(self)))
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
        DynObsBorrow::from_dyn(Rc::new(DynamicObs(ObsBorrow(self))))
    }
}
pub trait ObservableRef: 'static {
    type Item: ?Sized;
    fn with<U>(&self, f: impl FnOnce(&Self::Item, &BindContext) -> U, cx: &BindContext) -> U;

    fn into_dyn(self) -> DynObsRef<Self::Item>
    where
        Self: Sized,
    {
        DynObsRef::from_dyn(Rc::new(DynamicObs(ObsRef(self))))
    }
}

pub trait Observer: 'static {
    type Item;
    fn next(&mut self, value: Self::Item);
}
pub fn observer<T: 'static>(f: impl FnMut(T) + 'static) -> impl Observer<Item = T> {
    struct FnObserver<T, F> {
        f: F,
        _phantom: PhantomData<fn(T)>,
    }
    impl<T: 'static, F: FnMut(T) + 'static> Observer for FnObserver<T, F> {
        type Item = T;
        fn next(&mut self, value: Self::Item) {
            (self.f)(value)
        }
    }
    FnObserver {
        f,
        _phantom: PhantomData,
    }
}

pub trait IntoObserver {
    type Observer: Observer<Item = Self::Item>;
    type Item;

    fn into_observer(self) -> Self::Observer;
}
impl<O: Observer> IntoObserver for O {
    type Observer = Self;
    type Item = <Self as Observer>::Item;

    fn into_observer(self) -> Self::Observer {
        self
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

pub fn subscribe(mut f: impl FnMut(&BindContext) + 'static) -> Subscription {
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
