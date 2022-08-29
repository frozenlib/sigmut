use crate::*;
use std::rc::Rc;

pub trait DynObservable {
    type Item: ?Sized;

    fn d_with_dyn<'a>(&self, o: ObsContext<'a, '_, '_, Self::Item>) -> ObsRet<'a>;
    fn d_get(&self, bc: &mut BindContext) -> <Self::Item as ToOwned>::Owned
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

    fn d_with_dyn<'a>(&self, o: ObsContext<'a, '_, '_, Self::Item>) -> ObsRet<'a> {
        self.with_dyn(o)
    }

    fn d_get(&self, bc: &mut BindContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        self.get(bc)
    }
}
impl<T: ?Sized> Observable for &dyn DynObservable<Item = T> {
    type Item = T;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        bc: &mut BindContext,
    ) -> U {
        ObsCallback::with(|cb| self.d_with_dyn(cb.context(bc)), f)
    }
}

pub(super) trait DynObservableInner: 'static {
    type Item: ?Sized;
    fn d_with_dyn<'a>(self: Rc<Self>, oc: ObsContext<'a, '_, '_, Self::Item>) -> ObsRet<'a>;
}

impl<S> DynObservableInner for S
where
    S: 'static,
    Rc<S>: Observable,
{
    type Item = <Rc<S> as Observable>::Item;

    fn d_with_dyn<'a>(self: Rc<Self>, oc: ObsContext<'a, '_, '_, Self::Item>) -> ObsRet<'a> {
        self.with_dyn(oc)
    }
}
