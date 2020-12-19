use slabmap::SlabMap;

use super::*;
use crate::*;
use std::{cell::RefCell, rc::Rc};

pub trait Collect: 'static {
    type Input;
    type Output;
    type Key;
    fn insert(&mut self, value: Self::Input) -> (Self::Key, bool);
    fn remove(&mut self, key: Self::Key) -> bool;
    fn set(&mut self, key: Self::Key, value: Self::Input) -> (Self::Key, bool);
    fn collect(&self) -> Self::Output;
}

pub trait ObservableCollect {
    type Observer: Observer<Self::Item>;
    type Item;

    fn insert(&self, value: Self::Item) -> Self::Observer;
}

pub struct ObsCollector<T>(Rc<ObsCollectorData<T>>);
struct ObsCollectorData<T> {
    collector: RefCell<T>,
    sinks: BindSinks,
}

pub struct ObsCollectorObserver<T: Collect> {
    collector: Rc<ObsCollectorData<T>>,
    key: Option<T::Key>,
}
impl<T: Collect> ObsCollector<T> {
    pub fn as_dyn(&self) -> DynObs<T::Output> {
        DynObs::from_dyn_source(self.0.clone())
    }
    pub fn as_dyn_ref(&self) -> DynObsRef<T::Output> {
        self.as_dyn().as_ref()
    }
    pub fn obs(&self) -> Obs<impl Observable<Item = T::Output> + Clone> {
        Obs(self.clone())
    }
    pub fn obs_ref(&self) -> ObsRef<impl ObservableRef<Item = T::Output> + Clone> {
        self.obs().as_ref()
    }
}
impl<T: Collect> Observable for ObsCollector<T> {
    type Item = T::Output;
    fn get(&self, cx: &BindContext) -> Self::Item {
        self.0.clone().get(cx)
    }
}

impl<T: Collect> ObservableCollect for ObsCollector<T> {
    type Observer = ObsCollectorObserver<T>;
    type Item = T::Input;
    fn insert(&self, value: Self::Item) -> Self::Observer {
        let (key, is_modified) = self.0.collector.borrow_mut().insert(value);
        if is_modified {
            Runtime::notify_defer(self.0.clone());
        }
        ObsCollectorObserver {
            collector: self.0.clone(),
            key: Some(key),
        }
    }
}
impl<T> Clone for ObsCollector<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T: Collect> ObsCollectorData<T> {
    pub fn get(self: Rc<Self>, cx: &BindContext) -> T::Output {
        let value = self.collector.borrow().collect();
        cx.bind(self.clone());
        value
    }
}
impl<T: Collect> DynamicObservableSource for ObsCollectorData<T> {
    type Item = T::Output;

    fn dyn_get(self: Rc<Self>, cx: &BindContext) -> Self::Item {
        self.get(cx)
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRefSource<Item = Self::Item>> {
        self
    }
}
impl<T: Collect> DynamicObservableRefSource for ObsCollectorData<T> {
    type Item = T::Output;
    fn dyn_with(self: Rc<Self>, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
        f(&self.get(cx), cx)
    }
}
impl<T: 'static> BindSource for ObsCollectorData<T> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}

impl<T: Collect> Observer<T::Input> for ObsCollectorObserver<T> {
    fn next(&mut self, value: T::Input) {
        let (key, is_modified) = self
            .collector
            .collector
            .borrow_mut()
            .set(self.key.take().unwrap(), value);
        self.key = Some(key);
        if is_modified {
            Runtime::notify_defer(self.collector.clone());
        }
    }
}
impl<T: Collect> Drop for ObsCollectorObserver<T> {
    fn drop(&mut self) {
        if self
            .collector
            .collector
            .borrow_mut()
            .remove(self.key.take().unwrap())
        {
            Runtime::notify_defer(self.collector.clone());
        }
    }
}

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
