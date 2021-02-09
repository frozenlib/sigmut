use super::*;
use crate::collections::obs_list::*;
use crate::*;
use std::{ops::Index, rc::Rc, sync::Arc};

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub enum SourceList<T> {
    Empty,
    RcVec(Rc<Vec<T>>),
    RcSlice(Rc<[T]>),
    ArcVec(Arc<Vec<T>>),
    ArcSlice(Arc<[T]>),
    Obs(ObsList<T>),
}
pub enum SourceListRef<'a, T: 'static> {
    Constant(&'a [T]),
    Obs(ObsListRef<'a, T>),
}
pub enum SourceListAge<T> {
    Initial,
    Current,
    Obs(ObsListAge<T>),
}
pub enum SourceListChanges<'a, T: 'static> {
    ConstantValues {
        values: &'a [T],
        index: usize,
    },
    ObsValues {
        values: &'a ObsListRef<'a, T>,
        index: usize,
    },
    ObsChanges(ObsListChanges<'a, T>),
}

impl<T: 'static> SourceList<T> {
    pub fn borrow(&self, cx: &mut BindContext) -> SourceListRef<T> {
        match self {
            SourceList::Empty => SourceListRef::Constant(&[]),
            SourceList::RcVec(s) => SourceListRef::Constant(&s),
            SourceList::RcSlice(s) => SourceListRef::Constant(&s),
            SourceList::ArcVec(s) => SourceListRef::Constant(&s),
            SourceList::ArcSlice(s) => SourceListRef::Constant(&s),
            SourceList::Obs(o) => SourceListRef::Obs(o.borrow(cx)),
        }
    }
}
impl<'a, T: 'static> SourceListRef<'a, T> {
    pub fn age(&self) -> SourceListAge<T> {
        match self {
            SourceListRef::Constant(_) => SourceListAge::Current,
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
    pub fn changes(&self, since: &SourceListAge<T>) -> SourceListChanges<T> {
        match self {
            SourceListRef::Constant(s) => match since {
                SourceListAge::Initial => SourceListChanges::from_values(s),
                SourceListAge::Current => SourceListChanges::from_values(&[]),
                SourceListAge::Obs(_) => panic!("mismatch source."),
            },
            SourceListRef::Obs(o) => match since {
                SourceListAge::Initial => SourceListChanges::ObsValues {
                    values: o,
                    index: 0,
                },
                SourceListAge::Current => SourceListChanges::from_values(&[]),
                SourceListAge::Obs(since) => SourceListChanges::ObsChanges(o.changes(since)),
            },
        }
    }
}
impl<T> SourceListAge<T> {
    pub fn new() -> Self {
        SourceListAge::Initial
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

impl<'a, T: 'static> SourceListChanges<'a, T> {
    fn from_values(values: &'a [T]) -> Self {
        Self::ConstantValues { values, index: 0 }
    }
}
impl<'a, T: 'static> Iterator for SourceListChanges<'a, T> {
    type Item = ListChange<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            SourceListChanges::ConstantValues { values, index } => {
                let result = ListChange {
                    value: values.get(*index)?,
                    index: *index,
                    kind: ListChangeKind::Insert,
                };
                *index += 1;
                Some(result)
            }
            SourceListChanges::ObsValues { values, index } => {
                let result = ListChange {
                    value: values.get(*index)?,
                    index: *index,
                    kind: ListChangeKind::Insert,
                };
                *index += 1;
                Some(result)
            }
            SourceListChanges::ObsChanges(o) => o.next(),
        }
    }
}

pub trait IntoSourceList<T> {
    fn into_source_list(self) -> SourceList<T>;
}

impl<T> IntoSourceList<T> for ObsList<T> {
    fn into_source_list(self) -> SourceList<T> {
        SourceList::Obs(self)
    }
}
impl<T> IntoSourceList<T> for Vec<T> {
    fn into_source_list(self) -> SourceList<T> {
        SourceList::RcVec(self.into())
    }
}
impl<T: Clone> IntoSourceList<T> for &[T] {
    fn into_source_list(self) -> SourceList<T> {
        SourceList::RcSlice(self.into())
    }
}

impl<T> IntoSourceList<T> for Rc<Vec<T>> {
    fn into_source_list(self) -> SourceList<T> {
        SourceList::RcVec(self)
    }
}

impl<T> IntoSourceList<T> for Rc<[T]> {
    fn into_source_list(self) -> SourceList<T> {
        SourceList::RcSlice(self)
    }
}
impl<T> IntoSourceList<T> for Arc<Vec<T>> {
    fn into_source_list(self) -> SourceList<T> {
        SourceList::ArcVec(self)
    }
}

impl<T> IntoSourceList<T> for Arc<[T]> {
    fn into_source_list(self) -> SourceList<T> {
        SourceList::ArcSlice(self)
    }
}
