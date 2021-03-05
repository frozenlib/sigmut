use crate::*;
use std::{any::Any, ops::Deref, rc::Rc};

pub trait Observable: 'static {
    type Item: ?Sized;
    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U;
    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        DynObs::from_dyn(Rc::new(DynamicObs(self)))
    }
}

impl<S: Observable> Observable for Rc<S> {
    type Item = S::Item;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        self.deref().with(f, cx)
    }
    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        DynObs::from_dyn_inner(self)
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
