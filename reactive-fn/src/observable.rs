use super::*;
use crate::scan::*;
use std::{any::Any, cell::Ref, future::Future, ops::Deref, rc::Rc};

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

pub fn subscribe(mut f: impl FnMut(&BindContext) + 'static) -> Subscription {
    Subscription(Some(FoldBy::new(
        (),
        fold_op(move |st, cx| {
            f(cx);
            st
        }),
    )))
}
