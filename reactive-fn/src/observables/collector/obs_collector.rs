use observables::IntoObserver;

use crate::*;
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
pub struct ObsCollector<T>(Rc<ObsCollectorData<T>>);

#[derive(Default)]
struct ObsCollectorData<T> {
    collector: RefCell<T>,
    sinks: BindSinks,
}
pub trait Collect: 'static {
    type Input;
    type Output;
    type Key;
    fn insert(&mut self) -> (Self::Key, bool);
    fn remove(&mut self, key: Self::Key) -> bool;
    fn set(&mut self, key: Self::Key, value: Self::Input) -> (Self::Key, bool);
    fn collect(&self) -> Self::Output;
}

pub struct ObsCollectorObserver<T: Collect> {
    collector: Rc<ObsCollectorData<T>>,
    key: Option<T::Key>,
}
impl<T: Collect> ObsCollector<T> {
    pub fn new() -> Self
    where
        T: Default,
    {
        Default::default()
    }

    pub fn insert(&self) -> ObsCollectorObserver<T> {
        let (key, is_modified) = self.0.collector.borrow_mut().insert();
        if is_modified {
            Runtime::spawn_notify(self.0.clone());
        }
        ObsCollectorObserver {
            collector: self.0.clone(),
            key: Some(key),
        }
    }

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
impl<T> Clone for ObsCollector<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T: Collect> IntoObserver for &ObsCollector<T> {
    type Observer = ObsCollectorObserver<T>;
    type Item = T::Input;

    fn into_observer(self) -> Self::Observer {
        self.insert()
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

impl<T: Collect> Observer for ObsCollectorObserver<T> {
    type Item = T::Input;
    fn next(&mut self, value: T::Input) {
        let (key, is_modified) = self
            .collector
            .collector
            .borrow_mut()
            .set(self.key.take().unwrap(), value);
        self.key = Some(key);
        if is_modified {
            Runtime::spawn_notify(self.collector.clone());
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
            Runtime::spawn_notify(self.collector.clone());
        }
    }
}
