use crate::*;
use std::{any::Any, iter::FusedIterator, ops::Index, rc::Rc};

pub(crate) trait DynamicObservableList<T> {
    fn borrow<'a>(
        &'a self,
        rs_self: &dyn Any,
        cx: &mut BindContext,
    ) -> Box<dyn DynamicObservableListRef<T> + 'a>;
}
pub(crate) trait DynamicObservableListRef<T> {
    fn age(&self) -> DynObsListAge;
    fn len(&self) -> usize;
    fn get(&self, index: usize) -> Option<&T>;
    fn changes(&self, since: &DynObsListAge, f: &mut dyn FnMut(ListChange<&T>));
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub struct DynObsList<T>(pub(crate) Rc<dyn DynamicObservableList<T>>);
pub struct DynObsListRef<'a, T>(Box<dyn DynamicObservableListRef<T> + 'a>);

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub enum DynObsListAge {
    Empty,
    Last,
    Obs(Rc<dyn Any>),
}

impl<T: 'static> DynObsList<T> {
    pub fn borrow<'a>(&'a self, cx: &mut BindContext) -> DynObsListRef<'a, T> {
        DynObsListRef(self.0.borrow(&self.0, cx))
    }
}
impl<T> DynObsListRef<'_, T> {
    pub fn age(&self) -> DynObsListAge {
        self.0.age()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, index: usize) -> Option<&T> {
        self.0.get(index)
    }
    pub fn changes(&self, since: &DynObsListAge, f: &mut dyn FnMut(ListChange<&T>)) {
        self.0.changes(since, f)
    }
    pub fn iter(&self) -> Iter<T> {
        Iter::new(self)
    }
}
impl<T> Index<usize> for DynObsListRef<'_, T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("out of index.")
    }
}
impl<'a, T> IntoIterator for &'a DynObsListRef<'_, T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct Iter<'a, T> {
    s: &'a DynObsListRef<'a, T>,
    index: usize,
    s_len: usize,
}

impl<'a, T> Iter<'a, T> {
    fn new(s: &'a DynObsListRef<T>) -> Self {
        Self {
            s,
            index: 0,
            s_len: s.len(),
        }
    }
}
impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let value = self.s.get(self.index)?;
        self.index += 1;
        Some(value)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.s_len - self.index;
        (len, Some(len))
    }
}
impl<'a, T> ExactSizeIterator for Iter<'a, T> {}
impl<'a, T> FusedIterator for Iter<'a, T> {}
