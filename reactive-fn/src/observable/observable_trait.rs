use std::rc::Rc;

use super::{Consumed, ObsSink};
use crate::{core::ObsContext, ObsCallback};

/// Observable value.
pub trait Observable {
    type Item: ?Sized;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, oc: &mut ObsContext) -> U;

    #[inline]
    fn get_to<'cb>(&self, s: ObsSink<'cb, '_, '_, Self::Item>) -> Consumed<'cb> {
        self.with(|value, oc| s.cb.ret(value, oc), s.oc)
    }

    #[inline]
    fn get(&self, oc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        self.with(|value, _| value.to_owned(), oc)
    }
}

pub trait RcObservable {
    type Item: ?Sized;

    fn rc_with<U>(
        self: &Rc<Self>,
        f: impl FnOnce(&Self::Item, &mut ObsContext) -> U,
        oc: &mut ObsContext,
    ) -> U;

    #[inline]
    fn rc_get_to<'cb>(self: &Rc<Self>, s: ObsSink<'cb, '_, '_, Self::Item>) -> Consumed<'cb> {
        self.rc_with(|value, oc| s.cb.ret(value, oc), s.oc)
    }

    #[inline]
    fn rc_get(self: &Rc<Self>, oc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        self.rc_with(|value, _| value.to_owned(), oc)
    }
}
impl<O: RcObservable> Observable for Rc<O> {
    type Item = O::Item;

    #[inline]
    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, oc: &mut ObsContext) -> U {
        self.rc_with(f, oc)
    }

    #[inline]
    fn get_to<'cb>(&self, s: ObsSink<'cb, '_, '_, Self::Item>) -> Consumed<'cb> {
        self.rc_get_to(s)
    }

    #[inline]
    fn get(&self, oc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        self.rc_get(oc)
    }
}

pub trait DynObservable {
    type Item: ?Sized;
    fn dyn_get_to<'cb>(&self, s: ObsSink<'cb, '_, '_, Self::Item>) -> Consumed<'cb>;
    fn dyn_get(&self, oc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned;
}

impl<O> DynObservable for O
where
    O: Observable,
{
    type Item = O::Item;

    #[inline]
    fn dyn_get_to<'cb>(&self, s: ObsSink<'cb, '_, '_, Self::Item>) -> Consumed<'cb> {
        self.get_to(s)
    }

    #[inline]
    fn dyn_get(&self, oc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        self.get(oc)
    }
}

impl<T: ?Sized + Observable> Observable for &T {
    type Item = T::Item;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, oc: &mut ObsContext) -> U {
        <T as Observable>::with(*self, f, oc)
    }
    fn get_to<'cb>(&self, s: ObsSink<'cb, '_, '_, Self::Item>) -> Consumed<'cb> {
        <T as Observable>::get_to(*self, s)
    }
    fn get(&self, oc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        <T as Observable>::get(*self, oc)
    }
}

impl<T: ?Sized + 'static> Observable for dyn DynObservable<Item = T> {
    type Item = T;

    #[inline]
    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, oc: &mut ObsContext) -> U {
        ObsCallback::with(|cb| self.dyn_get_to(cb.context(oc)), f)
    }
}
