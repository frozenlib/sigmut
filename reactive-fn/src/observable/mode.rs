use super::{Consumed, ObsSink, Observable};
use crate::core::{
    dependency_node::{Compute, DependencyNode, DependencyNodeSettings},
    ComputeContext, ObsContext,
};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Mode {
    pub is_hasty: bool,
    pub is_keep: bool,
    pub is_hot: bool,
}

pub(crate) struct SetMode<O> {
    o: O,
    is_keep: bool,
}

impl<O: Observable + 'static> SetMode<O> {
    pub(crate) fn new(o: O, mode: Mode) -> Rc<DependencyNode<Self>> {
        let Mode {
            is_hasty,
            is_keep,
            is_hot,
        } = mode;
        DependencyNode::new(
            SetMode { o, is_keep },
            DependencyNodeSettings {
                is_hasty,
                is_hot,
                is_modify_always: true,
            },
        )
    }
}

impl<O: Observable + 'static> SetMode<O> {}
impl<O: Observable + 'static> Compute for SetMode<O> {
    fn compute(&mut self, cc: ComputeContext) -> bool {
        self.o.with(|_value, _oc| {}, cc.oc());
        true
    }

    fn discard(&mut self) -> bool {
        !self.is_keep
    }
}

impl<O: Observable + 'static> Observable for DependencyNode<SetMode<O>> {
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
