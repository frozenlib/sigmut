use std::rc::Rc;

use super::*;

pub type SourceList<T> = DynObsList<T>;

impl<T: 'static> From<ObsListCell<T>> for SourceList<T> {
    fn from(s: ObsListCell<T>) -> Self {
        s.as_dyn()
    }
}
impl<T: 'static> From<Vec<T>> for SourceList<T> {
    fn from(values: Vec<T>) -> Self {
        SourceList::from_vec(values)
    }
}
impl<T: 'static> From<Rc<Vec<T>>> for SourceList<T> {
    fn from(values: Rc<Vec<T>>) -> Self {
        SourceList::from_rc_vec(values)
    }
}
impl<'a, T: 'static> From<&'a Rc<Vec<T>>> for SourceList<T> {
    fn from(values: &'a Rc<Vec<T>>) -> Self {
        SourceList::from_rc_vec(values.clone())
    }
}
