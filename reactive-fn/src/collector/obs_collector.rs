use super::*;
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

struct ObsCollectorObserver<T: Collect> {
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

    pub fn insert(&self) -> impl Observer<T::Input> {
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
        self.obs().into_dyn()
    }
    pub fn obs(&self) -> Obs<impl Observable<Item = T::Output> + Clone> {
        Obs(self.clone())
    }
}
impl<T: Collect> Observable for ObsCollector<T> {
    type Item = T::Output;
    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        f(&self.0.clone().get(cx), cx)
    }
}
impl<T> Clone for ObsCollector<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Collect> ObsCollectorData<T> {
    pub fn get(self: Rc<Self>, cx: &mut BindContext) -> T::Output {
        let value = self.collector.borrow().collect();
        cx.bind(self);
        value
    }
}
impl<T: Collect> DynamicObservableInner for ObsCollectorData<T> {
    type Item = T::Output;
    fn dyn_with(
        self: Rc<Self>,
        f: &mut dyn FnMut(&Self::Item, &mut BindContext),
        cx: &mut BindContext,
    ) {
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
impl<T: Collect> Sink<T::Input> for ObsCollector<T> {
    fn connect(self, value: T::Input) -> DynObserver<T::Input> {
        (&self).connect(value)
    }
}
impl<T: Collect> Sink<T::Input> for &ObsCollector<T> {
    fn connect(self, value: T::Input) -> DynObserver<T::Input> {
        let mut o = self.insert();
        o.next(value);
        o.into_dyn()
    }
}
