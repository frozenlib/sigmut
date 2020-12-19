use super::*;
use slabmap::SlabMap;

pub type ObsAnyCollector = ObsCollector<AnyCollector>;
pub struct AnyCollector {
    count: usize,
}
impl Collect for AnyCollector {
    type Input = bool;
    type Output = bool;
    type Key = bool;

    fn insert(&mut self, value: Self::Input) -> (Self::Key, bool) {
        if value {
            self.count += 1;
            (true, self.count == 1)
        } else {
            (false, false)
        }
    }

    fn remove(&mut self, key: Self::Key) -> bool {
        if key {
            self.count -= 1;
            self.count == 0
        } else {
            false
        }
    }

    fn set(&mut self, key: Self::Key, value: Self::Input) -> (Self::Key, bool) {
        match (key, value) {
            (true, false) => (false, self.remove(key)),
            (false, true) => self.insert(true),
            _ => (false, value),
        }
    }

    fn collect(&self) -> Self::Output {
        self.count != 0
    }
}
pub type ObsSomeCollector<T> = ObsCollector<SomeCollector<T>>;
pub struct SomeCollector<T>(SlabMap<T>);

impl<T: Clone + 'static> SomeCollector<T> {
    fn is_result(&self, key: usize) -> bool {
        self.0.keys().next() == Some(key)
    }
}
impl<T: Clone + 'static> Collect for SomeCollector<T> {
    type Input = Option<T>;
    type Output = Option<T>;
    type Key = Option<usize>;

    fn insert(&mut self, value: Self::Input) -> (Self::Key, bool) {
        if let Some(value) = value {
            let key = self.0.insert(value);
            (Some(key), self.is_result(key))
        } else {
            (None, false)
        }
    }

    fn remove(&mut self, key: Self::Key) -> bool {
        if let Some(key) = key {
            let is_modified = self.is_result(key);
            self.0.remove(key);
            is_modified
        } else {
            false
        }
    }

    fn set(&mut self, key: Self::Key, value: Self::Input) -> (Self::Key, bool) {
        let is_modified0 = self.remove(key);
        let (key, is_modified1) = self.insert(value);
        (key, is_modified0 | is_modified1)
    }

    fn collect(&self) -> Self::Output {
        self.0.values().next().cloned()
    }
}
