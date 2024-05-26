use std::{
    cmp::{max, min},
    collections::VecDeque,
    ops::{Index, IndexMut},
};

#[derive(Debug)]
pub struct ISizeMap<T> {
    start: isize,
    values: VecDeque<T>,
    default: T,
}

impl<T: Default> ISizeMap<T> {
    pub fn new() -> Self {
        Self {
            start: 0,
            values: VecDeque::new(),
            default: T::default(),
        }
    }
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    pub fn end_index(&self) -> isize {
        self.start + self.values.len() as isize
    }
    pub fn get_mut(&mut self, index: isize) -> Option<&mut T> {
        if self.start <= index && (index - self.start) < self.values.len() as isize {
            Some(&mut self.values[(index - self.start) as usize])
        } else {
            None
        }
    }

    fn prepare(&mut self, start: isize, last: isize) {
        assert!(start <= last);
        if self.is_empty() {
            self.start = start;
            self.values.reserve((last + 1 - start) as usize);
            self.values.push_back(T::default());
        } else {
            let end = self.end_index();
            let len = self.len();
            let len_new = (max(last + 1, end) - min(self.start, start)) as usize;
            if len_new > len {
                self.values.reserve(len_new - len);
            }
        }
        while start < self.start {
            self.values.push_front(T::default());
            self.start -= 1;
        }
        while self.values.len() <= (last - self.start) as usize {
            self.values.push_back(T::default());
        }
    }
}
impl<T: Default> Index<isize> for ISizeMap<T> {
    type Output = T;
    fn index(&self, index: isize) -> &Self::Output {
        if self.start <= index && (index - self.start) < self.values.len() as isize {
            &self.values[(index - self.start) as usize]
        } else {
            &self.default
        }
    }
}
impl<T: Default> IndexMut<isize> for ISizeMap<T> {
    fn index_mut(&mut self, index: isize) -> &mut Self::Output {
        self.prepare(index, index);
        self.get_mut(index).unwrap()
    }
}
