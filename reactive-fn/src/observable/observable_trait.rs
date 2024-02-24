use std::rc::Rc;

use crate::core::{ObsContext, ObsRef};

/// Observable value.
pub trait Observable {
    type Item: ?Sized;

    fn borrow<'a, 'b: 'a>(&'a self, oc: &mut ObsContext<'b>) -> ObsRef<'a, Self::Item>;

    fn get(&self, oc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        self.borrow(oc).to_owned()
    }
}

impl<O: RcObservable> Observable for Rc<O> {
    type Item = O::Item;

    fn borrow<'a, 'b: 'a>(&'a self, oc: &mut ObsContext<'b>) -> ObsRef<'a, Self::Item> {
        self.clone().rc_borrow(self, oc)
    }

    fn get(&self, oc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        self.clone().rc_get(self, oc)
    }
}

impl<T: ?Sized + Observable> Observable for &T {
    type Item = T::Item;

    fn borrow<'a, 'b: 'a>(&'a self, oc: &mut ObsContext<'b>) -> ObsRef<'a, Self::Item> {
        (**self).borrow(oc)
    }

    fn get(&self, oc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        (**self).get(oc)
    }
}

pub trait RcObservable {
    type Item: ?Sized;

    fn rc_borrow<'a, 'b: 'a>(
        self: Rc<Self>,
        inner: &'a Self,
        oc: &mut ObsContext<'b>,
    ) -> ObsRef<'a, Self::Item>;

    fn rc_get(self: Rc<Self>, inner: &Self, oc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        self.rc_borrow(inner, oc).to_owned()
    }
}
