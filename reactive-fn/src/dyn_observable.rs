use crate::*;
use std::rc::Rc;

pub trait DynObservable {
    type Item: ?Sized;

    fn d_with_dyn<'a>(&self, o: ObsSink<'a, '_, '_, Self::Item>) -> Ret<'a>;
    fn d_get(&self, bc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned;

    fn d_into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized + 'static,
    {
        DynObs::new_dyn(Rc::new(self))
    }
    fn d_into_may(self) -> MayObs<Self::Item>
    where
        Self: Sized + 'static,
        Self::Item: Sized,
    {
        MayObs::Obs(self.d_into_dyn())
    }
}
impl<S: Observable> DynObservable for S {
    type Item = S::Item;

    fn d_with_dyn<'a>(&self, o: ObsSink<'a, '_, '_, Self::Item>) -> Ret<'a> {
        self.with_dyn(o)
    }

    fn d_get(&self, bc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        self.get(bc)
    }

    fn d_into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized + 'static,
    {
        self.into_dyn()
    }

    fn d_into_may(self) -> MayObs<Self::Item>
    where
        Self: Sized + 'static,
        Self::Item: Sized,
    {
        self.into_may()
    }
}
impl<T: ?Sized> Observable for dyn DynObservable<Item = T> {
    type Item = T;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, bc: &mut ObsContext) -> U {
        ObsCallback::with(|cb| self.d_with_dyn(cb.context(bc)), f)
    }

    fn with_dyn<'a>(&self, o: ObsSink<'a, '_, '_, Self::Item>) -> Ret<'a> {
        self.d_with_dyn(o)
    }

    fn get(&self, bc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        self.d_get(bc)
    }
}

pub(super) trait DynObservableInner: 'static {
    type Item: ?Sized;
    fn d_with_dyn<'a>(self: Rc<Self>, oc: ObsSink<'a, '_, '_, Self::Item>) -> Ret<'a>;
    fn d_get(self: Rc<Self>, bc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        ObsCallback::with(
            |cb| self.d_with_dyn(cb.context(bc)),
            |value, _| value.to_owned(),
        )
    }
}

impl<S> DynObservableInner for S
where
    S: 'static,
    Rc<S>: Observable,
{
    type Item = <Rc<S> as Observable>::Item;

    fn d_with_dyn<'a>(self: Rc<Self>, oc: ObsSink<'a, '_, '_, Self::Item>) -> Ret<'a> {
        self.with_dyn(oc)
    }

    fn d_get(self: Rc<Self>, bc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        self.get(bc)
    }
}
