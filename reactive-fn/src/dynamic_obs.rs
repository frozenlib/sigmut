use crate::*;
use std::rc::Rc;

pub trait DynamicObservable: 'static {
    type Item: ?Sized;
    fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &mut BindContext), bc: &mut BindContext);
}
pub trait DynamicObservableInner: 'static {
    type Item: ?Sized;
    fn dyn_with(
        self: Rc<Self>,
        f: &mut dyn FnMut(&Self::Item, &mut BindContext),
        bc: &mut BindContext,
    );
}
pub struct DynamicObs<S>(pub S);
impl<S: Observable> DynamicObservable for DynamicObs<S> {
    type Item = S::Item;

    fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &mut BindContext), bc: &mut BindContext) {
        self.0.with(f, bc)
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

    fn dyn_with(
        self: Rc<Self>,
        f: &mut dyn FnMut(&Self::Item, &mut BindContext),
        bc: &mut BindContext,
    ) {
        self.with(f, bc)
    }
}
