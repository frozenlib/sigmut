use super::*;
use crate::*;
use std::{borrow::Borrow, ops::Deref};

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub struct SourceList<T>(DynObsList<T>);

impl<T> Deref for SourceList<T> {
    type Target = DynObsList<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, U> From<ObsListCell<T>> for SourceList<U>
where
    T: Borrow<U> + 'static,
    U: 'static,
{
    fn from(s: ObsListCell<T>) -> Self {
        Self(s.as_dyn().map_borrow())
    }
}
impl<T, U> From<DynObsList<T>> for SourceList<U>
where
    T: Borrow<U> + 'static,
    U: 'static,
{
    fn from(s: DynObsList<T>) -> Self {
        Self(s.map_borrow())
    }
}
