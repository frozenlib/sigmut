use crate::*;
use std::{ops::Deref, rc::Rc, sync::Arc};

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub enum SharedArray<T: 'static> {
    Empty,
    Slice(&'static [T]),
    RcSlice(Rc<[T]>),
    RcVec(Rc<Vec<T>>),
    ArcSlice(Arc<[T]>),
    ArcVec(Arc<Vec<T>>),
}

impl<T> Deref for SharedArray<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<T> SharedArray<T> {
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

impl<T> From<&'static [T]> for SharedArray<T> {
    fn from(s: &'static [T]) -> Self {
        Self::Slice(s)
    }
}
impl<T> From<Vec<T>> for SharedArray<T> {
    fn from(s: Vec<T>) -> Self {
        Self::RcVec(Rc::new(s))
    }
}
impl<T> From<Rc<Vec<T>>> for SharedArray<T> {
    fn from(s: Rc<Vec<T>>) -> Self {
        Self::RcVec(s)
    }
}
impl<T> From<Rc<[T]>> for SharedArray<T> {
    fn from(s: Rc<[T]>) -> Self {
        Self::RcSlice(s)
    }
}
impl<T> From<Arc<Vec<T>>> for SharedArray<T> {
    fn from(s: Arc<Vec<T>>) -> Self {
        Self::ArcVec(s)
    }
}
impl<T> From<Arc<[T]>> for SharedArray<T> {
    fn from(s: Arc<[T]>) -> Self {
        Self::ArcSlice(s)
    }
}
