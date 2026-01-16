use std::cmp::{max, min};

use derive_ex::Ex;

use super::isize_map::ISizeMap;

#[derive(Ex)]
#[derive_ex(Default)]
#[default(Self::new())]
pub struct Buckets<T> {
    buckets: ISizeMap<Option<Vec<T>>>,
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
    pub fn set_empty(&mut self) {
        self.start = isize::MAX;
        self.last = isize::MIN;
    }
    pub fn contains_bucket(&self, id: isize) -> bool {
        matches!(self.buckets.get(id), Some(Some(_)))
    }
    pub fn register_bucket(&mut self, id: isize) {
        let b = &mut self.buckets[id];
        if b.is_none() {
            *b = Some(Vec::new());
        }
    }
    #[must_use]
    pub fn try_push(&mut self, id: isize, item: T) -> bool {
        if let Some(Some(bucket)) = self.buckets.get_mut(id) {
            bucket.push(item);
            self.start = min(self.start, id);
            self.last = max(self.last, id);
            true
        } else {
            false
        }
    }
    pub fn drain(&mut self, id: Option<isize>, to: &mut Vec<T>) {
        if let Some(id) = id {
            if let Some(Some(bucket)) = self.buckets.get_mut(id) {
                to.append(bucket)
            }
            if self.start == id {
                self.start += 1;
            }
            if self.start > self.last {
                self.set_empty();
            }
        } else {
            for id in self.start..=self.last {
                if let Some(Some(bucket)) = &mut self.buckets.get_mut(id) {
                    to.append(bucket)
                }
            }
            self.set_empty();
        }
    }
}
