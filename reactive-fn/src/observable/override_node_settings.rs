use super::{Consumed, ObsSink, Observable};
use crate::core::{
    dependency_node::{Compute, DependencyNode, DependencyNodeSettings},
    ComputeContext, ObsContext,
};
use std::rc::Rc;

pub(crate) struct OverrideNodeSettings<O> {
    o: O,
    is_using: bool,
}

impl<O: Observable + 'static> OverrideNodeSettings<O> {
    pub(crate) fn new(
        o: O,
        is_flush: bool,
        is_using: bool,
        is_hot: bool,
    ) -> Rc<DependencyNode<Self>> {
        DependencyNode::new(
            OverrideNodeSettings { o, is_using },
            DependencyNodeSettings {
                is_flush,
                is_hot,
                is_modify_always: true,
            },
        )
    }
}

impl<O: Observable + 'static> OverrideNodeSettings<O> {}
impl<O: Observable + 'static> Compute for OverrideNodeSettings<O> {
    fn compute(&mut self, cc: &mut ComputeContext) -> bool {
        self.o.with(|_value, _oc| {}, cc.oc());
        true
    }

    fn discard(&mut self) -> bool {
        !self.is_using
    }
}

impl<O: Observable + 'static> Observable for DependencyNode<OverrideNodeSettings<O>> {
    type Item = O::Item;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, oc: &mut ObsContext) -> U {
        self.borrow().o.with(f, oc)
    }
    fn get_to<'cb>(&self, s: ObsSink<'cb, '_, '_, Self::Item>) -> Consumed<'cb> {
        self.borrow().o.get_to(s)
    }
    fn get(&self, oc: &mut ObsContext) -> <Self::Item as ToOwned>::Owned
    where
        Self::Item: ToOwned,
    {
        self.borrow().o.get(oc)
    }
}
