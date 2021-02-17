use super::*;
use crate::collections::cell::*;
use crate::*;
use std::ops::Index;

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub enum SourceList<T: 'static> {
    Constant(SharedArray<T>),
    Obs(ObsListCell<T>),
}
pub enum SourceListRef<'a, T: 'static> {
    Constant(&'a [T]),
    Obs(ObsListCellRef<'a, T>),
}

#[derive(Clone, PartialEq)]
pub enum SourceListAge<T> {
    Empty,
    Last,
    Obs(ObsListAge<T>),
}
pub enum SourceListChanges<'a, T: 'static> {
    Constant { values: &'a [T], index: usize },
    Obs(ObsListChanges<'a, T>),
}

impl<T: 'static> SourceList<T> {
    pub fn borrow(&self, cx: &mut BindContext) -> SourceListRef<T> {
        match self {
            SourceList::Constant(s) => SourceListRef::Constant(&s),
            SourceList::Obs(o) => SourceListRef::Obs(o.borrow(cx)),
        }
    }
}
impl<'a, T: 'static> SourceListRef<'a, T> {
    pub fn age(&self) -> SourceListAge<T> {
        match self {
            SourceListRef::Constant(_) => SourceListAge::Last,
            SourceListRef::Obs(o) => SourceListAge::Obs(o.age()),
        }
    }
    pub fn len(&self) -> usize {
        match self {
            SourceListRef::Constant(s) => s.len(),
            SourceListRef::Obs(o) => o.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, index: usize) -> Option<&T> {
        match self {
            SourceListRef::Constant(c) => c.get(index),
            SourceListRef::Obs(o) => o.get(index),
        }
    }
    pub fn iter(&self) -> Iter<T> {
        Iter::new(self)
    }
    pub fn changes(&self, since: &SourceListAge<T>) -> SourceListChanges<T> {
        match self {
            SourceListRef::Constant(s) => match since {
                SourceListAge::Empty => SourceListChanges::from_values(s),
                SourceListAge::Last => SourceListChanges::from_values(&[]),
                SourceListAge::Obs(_) => panic!("mismatch source."),
            },
            SourceListRef::Obs(o) => match since {
                SourceListAge::Empty => SourceListChanges::Obs(o.changes(None)),
                SourceListAge::Last => SourceListChanges::from_values(&[]),
                SourceListAge::Obs(since) => SourceListChanges::Obs(o.changes(Some(since))),
            },
        }
    }
}
impl<T> SourceListAge<T> {
    pub fn new() -> Self {
        SourceListAge::Empty
    }
    pub fn is_last(self) -> bool {
        matches!(self, Self::Last)
    }
}
impl<T> Default for SourceListAge<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, T: 'static> Index<usize> for SourceListRef<'a, T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("out of index.")
    }
}

impl<'a, T: 'static> IntoIterator for &'a SourceListRef<'a, T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
pub struct Iter<'a, T: 'static> {
    s: &'a SourceListRef<'a, T>,
    index: usize,
    len: usize,
}
impl<'a, T: 'static> Iter<'a, T> {
    fn new(s: &'a SourceListRef<'a, T>) -> Self {
        Self {
            s,
            index: 0,
            len: s.len(),
        }
    }
}
impl<'a, T: 'static> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let value = self.s.get(self.index)?;
        self.index += 1;
        Some(value)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len - self.index;
        (len, Some(len))
    }
}

impl<'a, T: 'static> SourceListChanges<'a, T> {
    fn from_values(values: &'a [T]) -> Self {
        Self::Constant { values, index: 0 }
    }
}
impl<'a, T: 'static> Iterator for SourceListChanges<'a, T> {
    type Item = ListChange<&'a T>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            SourceListChanges::Constant { values, index } => {
                let result = ListChange {
                    value: values.get(*index)?,
                    index: *index,
                    kind: ListChangeKind::Insert,
                };
                *index += 1;
                Some(result)
            }
            SourceListChanges::Obs(o) => o.next(),
        }
    }
}

pub trait IntoSourceList<T> {
    fn into_source_list(self) -> SourceList<T>;
}

impl<T> IntoSourceList<T> for ObsListCell<T> {
    fn into_source_list(self) -> SourceList<T> {
        SourceList::Obs(self)
    }
}
impl<T: 'static, S: Into<SharedArray<T>>> IntoSourceList<T> for S {
    fn into_source_list(self) -> SourceList<T> {
        SourceList::Constant(self.into())
    }
}
