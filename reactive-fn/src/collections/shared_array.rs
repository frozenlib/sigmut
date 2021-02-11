use crate::*;
use std::{ops::Deref, rc::Rc, sync::Arc};

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
#[non_exhaustive]
pub enum SharedArray<'a, T: 'a> {
    Empty,
    Slice(&'a [T]),
    RcSlice(Rc<[T]>),
    RcVec(Rc<Vec<T>>),
    ArcSlice(Arc<[T]>),
    ArcVec(Arc<Vec<T>>),
}

impl<T> Deref for SharedArray<'_, T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<T> SharedArray<'_, T> {
    pub fn as_slice(&self) -> &[T] {
        match self {
            SharedArray::Empty => &[],
            SharedArray::Slice(s) => s,
            SharedArray::RcSlice(rc) => &rc,
            SharedArray::RcVec(rc) => &rc,
            SharedArray::ArcSlice(arc) => &arc,
            SharedArray::ArcVec(arc) => &arc,
        }
    }
}

impl<'a, T> From<&'a [T]> for SharedArray<'a, T> {
    fn from(s: &'a [T]) -> Self {
        Self::Slice(s)
    }
}
impl<'a, T> From<Vec<T>> for SharedArray<'a, T> {
    fn from(s: Vec<T>) -> Self {
        Self::RcVec(Rc::new(s))
    }
}
impl<'a, T> From<Rc<Vec<T>>> for SharedArray<'a, T> {
    fn from(s: Rc<Vec<T>>) -> Self {
        Self::RcVec(s)
    }
}
impl<'a, T> From<Rc<[T]>> for SharedArray<'a, T> {
    fn from(s: Rc<[T]>) -> Self {
        Self::RcSlice(s)
    }
}
impl<'a, T> From<Arc<Vec<T>>> for SharedArray<'a, T> {
    fn from(s: Arc<Vec<T>>) -> Self {
        Self::ArcVec(s)
    }
}
impl<'a, T> From<Arc<[T]>> for SharedArray<'a, T> {
    fn from(s: Arc<[T]>) -> Self {
        Self::ArcSlice(s)
    }
}
