use std::{
    cmp::{max, min},
    collections::VecDeque,
};

use derive_ex::Ex;

use super::isize_map::ISizeMap;

#[derive(Ex)]
#[derive_ex(Default)]
#[default(Self::new())]
pub struct Buckets<T> {
    buckets: ISizeMap<VecDeque<T>>,
    start: isize,
    last: isize,
}
impl<T> Buckets<T> {
    pub fn new() -> Self {
        Self {
            buckets: ISizeMap::new(),
            start: isize::MAX,
            last: isize::MIN,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.start == isize::MAX
    }
    pub fn ids(&self) -> impl Iterator<Item = isize> + '_ {
        (self.start..=self.last).filter(|&id| !self.buckets[id].is_empty())
    }
    pub fn set_empty(&mut self) {
        self.start = isize::MAX;
        self.last = isize::MIN;
    }
    pub fn push(&mut self, id: isize, item: T) {
        self.buckets[id].push_back(item);
        self.start = min(self.start, id);
        self.last = max(self.last, id);
    }
    pub fn pop_front(&mut self, id: isize) -> Option<T> {
        let item = self.buckets.get_mut(id)?.pop_front()?;
        self.update_bounds(id);
        Some(item)
    }
    pub fn drain(&mut self, id: Option<isize>, to: &mut Vec<T>) {
        if let Some(id) = id {
            if let Some(bucket) = self.buckets.get_mut(id) {
                to.extend(bucket.drain(..));
            }
            self.update_bounds(id);
        } else {
            for id in self.start..=self.last {
                if let Some(bucket) = &mut self.buckets.get_mut(id) {
                    to.extend(bucket.drain(..));
                }
            }
            self.set_empty();
        }
    }

    fn update_bounds(&mut self, emptied_id: isize) {
        if self.start == emptied_id {
            while self.start <= self.last && self.buckets[self.start].is_empty() {
                self.start += 1;
            }
        }
        if self.last == emptied_id {
            while self.start <= self.last && self.buckets[self.last].is_empty() {
                self.last -= 1;
            }
        }
        if self.start > self.last {
            self.set_empty();
        }
    }
}

#[cfg(test)]
mod tests;
