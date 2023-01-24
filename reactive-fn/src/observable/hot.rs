use super::{Consumed, ObsSink, Observable};
use crate::core::{
    dependency_node::{Compute, DependencyNode, DependencyNodeSettings},
    ComputeContext, ObsContext,
};
use std::rc::Rc;

pub(crate) struct RawHot<O>(O);

impl<O: Observable + 'static> RawHot<O> {
    pub(crate) fn new(o: O) -> Rc<DependencyNode<Self>> {
        DependencyNode::new(
            RawHot(o),
            DependencyNodeSettings {
                is_flush: false,
                is_hot: true,
                is_modify_always: true,
            },
        )
    }
}

impl<O: Observable + 'static> RawHot<O> {}
impl<O: Observable + 'static> Compute for RawHot<O> {
    fn compute(&mut self, cc: &mut ComputeContext) -> bool {
        self.0.with(|_value, _oc| {}, cc.oc());
        true
    }
}

impl<O: Observable + 'static> Observable for DependencyNode<RawHot<O>> {
    type Item = O::Item;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, oc: &mut ObsContext) -> U {
        self.borrow().0.with(f, oc)
    }
    fn get_to<'cb>(&self, s: ObsSink<'cb, '_, '_, Self::Item>) -> Consumed<'cb> {
        self.borrow().0.get_to(s)
    }
    fn get(&self, oc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        self.borrow().0.get(oc)
    }
}
