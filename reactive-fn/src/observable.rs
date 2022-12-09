use crate::*;
use std::{any::Any, ops::Deref, rc::Rc};

pub trait Observable {
    type Item: ?Sized;
    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, bc: &mut ObsContext) -> U;
    fn with_dyn<'a>(&self, o: ObsSink<'a, '_, '_, Self::Item>) -> Ret<'a> {
        self.with(|value, bc| o.cb.ret(value, bc), o.bc)
    }
    fn with_head<U>(&self, f: impl FnOnce(&Self::Item) -> U) -> U {
        ObsContext::null(|bc| self.with(|value, _| f(value), bc))
    }

    fn get(&self, bc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        self.with(|value, _| value.to_owned(), bc)
    }
    fn get_head(&self) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        ObsContext::null(|bc| self.get(bc))
    }

    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized + 'static,
    {
        DynObs::new_dyn(Rc::new(self))
    }
    fn into_may(self) -> MayObs<Self::Item>
    where
        Self: Sized + 'static,
        Self::Item: Sized,
    {
        MayObs::Obs(self.into_dyn())
    }
}

impl<S: Observable + 'static> Observable for Rc<S> {
    type Item = S::Item;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, bc: &mut ObsContext) -> U {
        self.deref().with(f, bc)
    }
    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        DynObs::new_dyn_inner(self)
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

pub(crate) trait Map<I: ?Sized>: 'static {
    type Output: ?Sized;
    fn map<U>(&self, value: &I, f: impl FnOnce(&Self::Output) -> U) -> U;
}
pub(crate) struct MapId;
impl<T: ?Sized> Map<T> for MapId {
    type Output = T;
    fn map<U>(&self, value: &T, f: impl FnOnce(&Self::Output) -> U) -> U {
        f(value)
    }
}

pub(crate) struct MapValue<F>(pub F);
impl<F: Fn(&I) -> O + 'static, I: ?Sized, O> Map<I> for MapValue<F> {
    type Output = O;
    fn map<U>(&self, value: &I, f: impl FnOnce(&Self::Output) -> U) -> U {
        f(&(self.0)(value))
    }
}
pub(crate) struct MapRef<F>(pub F);
impl<F: Fn(&I) -> &O + 'static, I: ?Sized, O: ?Sized> Map<I> for MapRef<F> {
    type Output = O;
    fn map<U>(&self, value: &I, f: impl FnOnce(&Self::Output) -> U) -> U {
        f((self.0)(value))
    }
}
