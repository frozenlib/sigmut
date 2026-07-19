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
    len: usize,
    start: isize,
    last: isize,
}
impl<T> Buckets<T> {
    pub fn new() -> Self {
        Self {
            buckets: ISizeMap::new(),
            len: 0,
            start: isize::MAX,
            last: isize::MIN,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn set_empty(&mut self) {
        self.len = 0;
        self.start = isize::MAX;
        self.last = isize::MIN;
    }
    pub fn push(&mut self, id: isize, item: T) {
        self.buckets[id].push_back(item);
        self.len += 1;
        self.start = min(self.start, id);
        self.last = max(self.last, id);
    }
    pub fn pop_front(&mut self, id: isize) -> Option<T> {
        let item = self.buckets.get_mut(id)?.pop_front()?;
        self.len -= 1;
        self.update_bounds(id);
        Some(item)
    }
    pub fn drain(&mut self, id: Option<isize>, to: &mut Vec<T>) {
        if let Some(id) = id {
            if let Some(bucket) = self.buckets.get_mut(id) {
                self.len -= bucket.len();
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
        if self.is_empty() {
            self.set_empty();
            return;
        }
        if self.start == emptied_id {
            while self.buckets[self.start].is_empty() {
                self.start += 1;
            }
        }
        if self.last == emptied_id {
            while self.buckets[self.last].is_empty() {
                self.last -= 1;
            }
        }
    }
}
