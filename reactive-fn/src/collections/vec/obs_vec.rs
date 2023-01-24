use crate::ObsContext;
use derive_ex::derive_ex;
use std::{cmp::min, iter::FusedIterator, ops::Index, rc::Rc};

use super::{ObsVecItemsMut, RawScan};

pub(crate) trait ObservableVec {
    type Item: ?Sized;

    fn increment_ref_count(&self) -> usize;
    fn decrement_ref_count(&self, age: Option<usize>);

    #[allow(clippy::type_complexity)]
    fn with(
        self: Rc<Self>,
        f: &mut dyn FnMut(&dyn ObservableVecItems<Item = Self::Item>, &mut ObsContext),
        oc: &mut ObsContext,
    );
}

pub(crate) trait ObservableVecItems {
    type Item: ?Sized;
    fn len(&self) -> usize;
    fn get(&self, index: usize) -> Option<&Self::Item>;
    fn changes(&self, age: usize, f: &mut dyn FnMut(ObsVecChange<Self::Item>));
}

#[derive_ex(Clone)]

enum RawObsVec<T: 'static> {
    Static(&'static [T]),
    Rc(Rc<dyn ObservableVec<Item = T>>),
}

#[derive_ex(Clone)]
pub struct ObsVec<T: 'static>(RawObsVec<T>);

impl<T: 'static> ObsVec<T> {
    pub(crate) fn from_rc(inner: Rc<dyn ObservableVec<Item = T>>) -> Self {
        Self(RawObsVec::Rc(inner))
    }
    pub(crate) fn from_static(inner: &'static [T]) -> Self {
        Self(RawObsVec::Static(inner))
    }
    pub fn from_scan(
        initial_state: impl IntoIterator<Item = T>,
        f: impl FnMut(&mut ObsVecItemsMut<T>, &mut ObsContext) + 'static,
    ) -> Self {
        Self::from_rc(RawScan::new(initial_state, f))
    }

    pub fn session(&self) -> ObsVecSession<T> {
        ObsVecSession {
            owner: self.clone(),
            age: None,
        }
    }

    fn increment_ref_count(&self) -> usize {
        match &self.0 {
            RawObsVec::Static(_) => 0,
            RawObsVec::Rc(inner) => inner.increment_ref_count(),
        }
    }
    fn decrement_ref_count(&self, age: Option<usize>) {
        match &self.0 {
            RawObsVec::Static(_) => {}
            RawObsVec::Rc(inner) => inner.decrement_ref_count(age),
        }
    }
    fn with(
        &self,
        f: &mut dyn FnMut(&dyn ObservableVecItems<Item = T>, &mut ObsContext),
        oc: &mut ObsContext,
    ) {
        match &self.0 {
            RawObsVec::Static(inner) => f(inner, oc),
            RawObsVec::Rc(inner) => inner.clone().with(f, oc),
        }
    }
}
impl<T> From<Vec<T>> for ObsVec<T> {
    fn from(value: Vec<T>) -> Self {
        Rc::new(value).into()
    }
}
impl<T> From<Rc<Vec<T>>> for ObsVec<T> {
    fn from(value: Rc<Vec<T>>) -> Self {
        Self::from_rc(value)
    }
}
impl<'a, T> From<&'a Rc<Vec<T>>> for ObsVec<T> {
    fn from(value: &'a Rc<Vec<T>>) -> Self {
        value.clone().into()
    }
}
impl<T> From<&'static [T]> for ObsVec<T> {
    fn from(value: &'static [T]) -> Self {
        Self::from_static(value)
    }
}
impl<T, const N: usize> From<[T; N]> for ObsVec<T> {
    fn from(value: [T; N]) -> Self {
        Self::from_rc(Rc::new(value))
    }
}

pub struct ObsVecSession<T: 'static> {
    owner: ObsVec<T>,
    age: Option<usize>,
}

impl<T> ObsVecSession<T> {
    pub fn read<U>(
        &mut self,
        f: impl FnOnce(ObsVecItems<T>, &mut ObsContext) -> U,
        oc: &mut ObsContext,
    ) -> U {
        let mut ret = None;
        let mut f = Some(f);
        let age = self.age;
        self.owner.with(
            &mut |r, oc| ret = Some((f.take().unwrap())(ObsVecItems { items: r, age }, oc)),
            oc,
        );
        self.owner.decrement_ref_count(self.age.take());
        self.age = Some(self.owner.increment_ref_count());
        ret.unwrap()
    }
}
impl<T> Drop for ObsVecSession<T> {
    fn drop(&mut self) {
        self.owner.decrement_ref_count(self.age);
    }
}

pub struct ObsVecItems<'a, T: ?Sized> {
    age: Option<usize>,
    items: &'a dyn ObservableVecItems<Item = T>,
}

impl<'a, T: ?Sized> ObsVecItems<'a, T> {
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, index: usize) -> Option<&T> {
        self.items.get(index)
    }
    pub fn changes(&self, mut f: impl FnMut(ObsVecChange<T>)) {
        if let Some(age) = self.age {
            self.items.changes(age, &mut f);
        } else {
            for index in 0..self.len() {
                f(ObsVecChange::Insert {
                    index,
                    new_value: self.get(index).unwrap(),
                });
            }
        }
    }
    pub fn iter(&self) -> ObsVecIter<T> {
        self.into_iter()
    }
}
impl<T: ?Sized> Index<usize> for ObsVecItems<'_, T> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("index out of bounds")
    }
}
impl<'a, T: ?Sized> IntoIterator for &'a ObsVecItems<'a, T> {
    type Item = &'a T;
    type IntoIter = ObsVecIter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        ObsVecIter::new(self.items)
    }
}
pub struct ObsVecIter<'a, T: ?Sized> {
    owner: &'a dyn ObservableVecItems<Item = T>,
    index: usize,
    end: usize,
}

impl<'a, T: ?Sized> ObsVecIter<'a, T> {
    pub(crate) fn new(owner: &'a dyn ObservableVecItems<Item = T>) -> Self {
        let end = owner.len();
        Self {
            owner,
            index: 0,
            end,
        }
    }
}
impl<'a, T: ?Sized> Iterator for ObsVecIter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        let index = self.index;
        if index < self.end {
            self.index += 1;
            self.owner.get(index)
        } else {
            None
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.end - self.index;
        (len, Some(len))
    }
    fn count(self) -> usize {
        self.end - self.index
    }
    fn last(self) -> Option<Self::Item> {
        if self.index < self.end {
            self.owner.get(self.end - 1)
        } else {
            None
        }
    }
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.index = min(self.index.saturating_add(n + 1), self.end);
        if self.index <= self.end {
            self.owner.get(self.index - 1)
        } else {
            None
        }
    }
}
impl<T: ?Sized> DoubleEndedIterator for ObsVecIter<'_, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.index < self.end {
            self.end -= 1;
            self.owner.get(self.end)
        } else {
            None
        }
    }
}
impl<T: ?Sized> FusedIterator for ObsVecIter<'_, T> {}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct IndexMapping {
    pub old_to_new: Vec<usize>,
    pub new_to_old: Vec<usize>,
}
impl IndexMapping {
    pub fn new(new_to_old: Vec<usize>) -> Self {
        let mut old_to_new = vec![usize::MAX; new_to_old.len()];
        for (new_index, &old_index) in new_to_old.iter().enumerate() {
            old_to_new[old_index] = new_index;
        }
        Self {
            old_to_new,
            new_to_old,
        }
    }
    pub fn apply_to<T>(&self, items: &mut [T]) {
        let mut old_to_new = self.old_to_new.clone();
        for old in 0..items.len() {
            loop {
                let new = old_to_new[old];
                if old == new {
                    break;
                }
                items.swap(old, new);
                old_to_new.swap(old, new);
            }
        }
    }
}

#[derive(Debug)]
#[derive_ex(Clone, Copy)]
pub enum ObsVecChange<'a, T: ?Sized> {
    Insert {
        index: usize,
        new_value: &'a T,
    },
    Remove {
        index: usize,
        old_value: &'a T,
    },
    Set {
        index: usize,
        new_value: &'a T,
        old_value: &'a T,
    },
    Move {
        old_index: usize,
        new_index: usize,
    },
    Swap {
        index: (usize, usize),
    },
    Sort(&'a IndexMapping),
}

impl<T> ObservableVec for Vec<T> {
    type Item = T;

    fn increment_ref_count(&self) -> usize {
        0
    }
    fn decrement_ref_count(&self, _age: Option<usize>) {}
    fn with(
        self: Rc<Self>,
        f: &mut dyn FnMut(&dyn ObservableVecItems<Item = Self::Item>, &mut ObsContext),
        oc: &mut ObsContext,
    ) {
        f(&self.as_slice(), oc)
    }
}
impl<T, const N: usize> ObservableVec for [T; N] {
    type Item = T;

    fn increment_ref_count(&self) -> usize {
        0
    }
    fn decrement_ref_count(&self, _age: Option<usize>) {}
    fn with(
        self: Rc<Self>,
        f: &mut dyn FnMut(&dyn ObservableVecItems<Item = Self::Item>, &mut ObsContext),
        oc: &mut ObsContext,
    ) {
        f(&self.as_slice(), oc)
    }
}

impl<T> ObservableVecItems for &[T] {
    type Item = T;
    fn len(&self) -> usize {
        <[T]>::len(self)
    }
    fn get(&self, index: usize) -> Option<&Self::Item> {
        <[T]>::get(self, index)
    }
    fn changes(&self, _age: usize, _f: &mut dyn FnMut(ObsVecChange<Self::Item>)) {}
}

impl<T, const N: usize> ObservableVecItems for [T; N] {
    type Item = T;
    fn len(&self) -> usize {
        N
    }
    fn get(&self, index: usize) -> Option<&Self::Item> {
        <[T]>::get(self, index)
    }
    fn changes(&self, _age: usize, _f: &mut dyn FnMut(ObsVecChange<Self::Item>)) {}
}
