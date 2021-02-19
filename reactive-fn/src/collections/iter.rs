use std::{iter::FusedIterator, ops::Index};

#[derive(Clone)]
pub struct IndexIter<S> {
    s: S,
    index: usize,
    end: usize,
}
impl<'a, S: Index<usize>> IndexIter<&'a S> {
    pub fn new(s: &'a S, index: usize, end: usize) -> Self {
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
