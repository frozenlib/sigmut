use std::rc::Rc;

use super::Observable;
use crate::{
    core::{ObsContext, ObsRef, ObsRefBuilder},
    helpers::dependency_node::{Compute, DependencyNode, DependencyNodeSettings},
};

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
    fn compute(&mut self, oc: &mut ObsContext) -> bool {
        self.o.borrow(oc.reset());
        true
    }

    fn discard(&mut self) -> bool {
        !self.is_keep
    }
}

impl<O: Observable + 'static> Observable for DependencyNode<SetMode<O>> {
    type Item = O::Item;

    fn borrow<'a, 'b: 'a>(&'a self, oc: &mut ObsContext<'b>) -> ObsRef<'a, Self::Item> {
        ObsRefBuilder::from_ref_cell(self.borrow(), oc)
            .map_ref(|o, oc, _| o.o.borrow(oc))
            .build()
    }
}
