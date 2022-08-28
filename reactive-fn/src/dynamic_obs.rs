use crate::*;
use std::rc::Rc;

pub trait DynamicObservable: 'static {
    type Item: ?Sized;
    fn dyn_with<'a>(&self, oc: ObsContext<'a, '_, '_, Self::Item>) -> ObsRet<'a>;
}
pub trait DynamicObservableInner: 'static {
    type Item: ?Sized;
    fn dyn_with<'a>(self: Rc<Self>, oc: ObsContext<'a, '_, '_, Self::Item>) -> ObsRet<'a>;
}
pub struct DynamicObs<S>(pub S);
impl<S: Observable> DynamicObservable for DynamicObs<S> {
    type Item = S::Item;

    fn dyn_with<'a>(&self, oc: ObsContext<'a, '_, '_, Self::Item>) -> ObsRet<'a> {
        self.0.with_dyn(oc)
    }
}
impl<S: Observable> Observable for DynamicObs<S> {
    type Item = S::Item;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        bc: &mut BindContext,
    ) -> U {
        self.0.with(f, bc)
    }
}

impl<S> DynamicObservableInner for S
where
    Rc<S>: Observable,
{
    type Item = <Rc<S> as Observable>::Item;

    fn dyn_with<'a>(self: Rc<Self>, oc: ObsContext<'a, '_, '_, Self::Item>) -> ObsRet<'a> {
        self.with_dyn(oc)
    }
}
