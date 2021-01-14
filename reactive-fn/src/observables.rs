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
mod source;
mod source_ref;
mod tail;

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
    source::*,
    source_ref::*,
    tail::*,
};
use self::{hot::*, into_stream::*, map_async::*, scan::*};
use crate::*;
use derivative::Derivative;
use std::{
    any::Any,
    borrow::Borrow,
    cell::{Ref, RefCell},
    future::Future,
    iter::once,
    marker::PhantomData,
    ops::Deref,
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
    fn into_obs_borrow(self) -> ObsBorrow<Self>
    where
        Self: Sized,
    {
        ObsBorrow(self)
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
    fn into_obs_ref(self) -> ObsRef<Self>
    where
        Self: Sized,
    {
        ObsRef(self)
    }
}

impl<S: Observable> Observable for Rc<S> {
    type Item = S::Item;

    fn get(&self, cx: &BindContext) -> Self::Item {
        self.deref().get(cx)
    }
}
impl<S: ObservableBorrow> ObservableBorrow for Rc<S> {
    type Item = S::Item;

    fn borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.deref().borrow(cx)
    }
}
impl<S: ObservableRef> ObservableRef for Rc<S> {
    type Item = S::Item;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &BindContext) -> U, cx: &BindContext) -> U {
        self.deref().with(f, cx)
    }
}

#[must_use]
#[derive(Clone, Default)]
pub struct Subscription(pub(crate) Option<Rc<dyn Any>>);

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
        fold_op(move |st, cx| {
            f(cx);
            st
        }),
    )))
}
