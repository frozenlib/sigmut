use std::{
    iter::FusedIterator,
    mem::transmute,
    ops::{Index, IndexMut},
};

#[derive(Clone)]
pub struct IndexIter<S> {
    s: S,
    index: usize,
    end: usize,
}
impl<'a, S: Index<usize>> IndexIter<&'a S> {
    pub(crate) fn new(s: &'a S, index: usize, end: usize) -> Self {
        Self { s, index, end }
    }
}
impl<'a, S: Index<usize>> Iterator for IndexIter<&'a S> {
    type Item = &'a S::Output;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.end {
            None
        } else {
            let value = &self.s[self.index];
            self.index += 1;
            Some(value)
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.end - self.index;
        (len, Some(len))
    }
}
impl<'a, S: Index<usize>> ExactSizeIterator for IndexIter<&'a S> {}
impl<'a, S: Index<usize>> FusedIterator for IndexIter<&'a S> {}
impl<'a, S: Index<usize>> DoubleEndedIterator for IndexIter<&'a S> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.index == self.end {
            None
        } else {
            let value = &self.s[self.end - 1];
            self.end -= 1;
            Some(value)
        }
    }
}

pub struct IndexMutIter<S> {
    s: S,
    index: usize,
    end: usize,
}
impl<'a, S: IndexMut<usize>> IndexMutIter<&'a mut S> {
    pub(crate) unsafe fn new(s: &'a mut S, index: usize, end: usize) -> Self {
        Self { s, index, end }
    }
}
impl<'a, S: IndexMut<usize>> Iterator for IndexMutIter<&'a mut S> {
    type Item = &'a mut S::Output;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.end {
            None
        } else {
            let value = unsafe { transmute(&mut self.s[self.index]) };
            self.index += 1;
            Some(value)
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.end - self.index;
        (len, Some(len))
    }
}
impl<'a, S: IndexMut<usize>> ExactSizeIterator for IndexMutIter<&'a mut S> {}
impl<'a, S: IndexMut<usize>> FusedIterator for IndexMutIter<&'a mut S> {}
impl<'a, S: IndexMut<usize>> DoubleEndedIterator for IndexMutIter<&'a mut S> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.index == self.end {
            None
        } else {
            let value = unsafe { transmute(&mut self.s[self.index]) };
            self.end -= 1;
            Some(value)
        }
    }
}
