use super::{CollectModify, Collector, ObsCollector};
use slabmap::SlabMap;

pub type ObsAnyCollector = ObsCollector<AnyCollector>;
#[derive(Debug, Default)]
pub struct AnyCollector {
    count: usize,
}
impl Collector for AnyCollector {
    type Input = bool;
    type Output = bool;
    type Key = bool;

    fn insert(&mut self) -> CollectModify<Self::Key> {
        CollectModify {
            key: false,
            is_modified: false,
        }
    }

    fn remove(&mut self, key: Self::Key) -> CollectModify {
        CollectModify::from_is_modified(if key {
            self.count -= 1;
            self.count == 0
        } else {
            false
        })
    }

    fn set(&mut self, key: Self::Key, value: Self::Input) -> CollectModify<Self::Key> {
        let is_modified = match (key, value) {
            (true, false) => self.remove(key).is_modified,
            (false, true) => {
                self.count += 1;
                self.count == 1
            }
            _ => false,
        };
        CollectModify {
            key: value,
            is_modified,
        }
    }

    fn collect(&self) -> Self::Output {
        self.count != 0
    }
}
pub type ObsSomeCollector<T> = ObsCollector<SomeCollector<T>>;

#[derive(Debug, Default)]
pub struct SomeCollector<T>(SlabMap<T>);

impl<T: Clone + 'static> SomeCollector<T> {
    fn is_result(&self, key: usize) -> bool {
        self.0.keys().next() == Some(key)
    }
}
impl<T: Clone + 'static> Collector for SomeCollector<T> {
    type Input = Option<T>;
    type Output = Option<T>;
    type Key = Option<usize>;

    fn insert(&mut self) -> CollectModify<Self::Key> {
        CollectModify {
            key: None,
            is_modified: false,
        }
    }

    fn remove(&mut self, key: Self::Key) -> CollectModify {
        CollectModify::from_is_modified(if let Some(key) = key {
            let is_modified = self.is_result(key);
            self.0.remove(key);
            is_modified
        } else {
            false
        })
    }

    fn set(&mut self, key: Self::Key, value: Self::Input) -> CollectModify<Self::Key> {
        let is_modified = self.remove(key).is_modified;
        if let Some(value) = value {
            let key = self.0.insert(value);
            CollectModify {
                key: Some(key),
                is_modified: is_modified || self.is_result(key),
            }
        } else {
            CollectModify {
                key: None,
                is_modified,
            }
        }
    }

    fn collect(&self) -> Self::Output {
        self.0.values().next().cloned()
    }
}
