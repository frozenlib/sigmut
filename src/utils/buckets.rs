use std::cmp::{max, min};

use derive_ex::Ex;

use super::isize_map::ISizeMap;

#[derive(Ex)]
#[derive_ex(Default)]
#[default(Self::new())]
pub struct Buckets<T> {
    buckets: ISizeMap<Vec<T>>,
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
    pub fn push(&mut self, id: isize, item: T) {
        self.buckets[id].push(item);
        self.start = min(self.start, id);
        self.last = max(self.last, id);
    }
    pub fn drain(&mut self, id: Option<isize>, to: &mut Vec<T>) {
        if let Some(id) = id {
            if let Some(bucket) = self.buckets.get_mut(id) {
                to.append(bucket)
            }
            if self.start == id {
                self.start += 1;
            }
            if self.start > self.last {
                self.set_empty();
            }
        } else {
            for index in self.start..=self.last {
                to.append(&mut self.buckets[index])
            }
            self.set_empty();
        }
    }
}
