use super::*;
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
pub struct ObsCollector<C>(Rc<ObsCollectorData<C>>);

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

pub struct ObsCollectorObserver<C: Collect> {
    collector: Rc<ObsCollectorData<C>>,
    key: Option<C::Key>,
}
impl<C: Collect> ObsCollector<C> {
    pub fn new() -> Self
    where
        C: Default,
    {
        Default::default()
    }

    fn insert(&self) -> ObsCollectorObserver<C> {
        let (key, is_modified) = self.0.collector.borrow_mut().insert();
        if is_modified {
            Runtime::spawn_notify(self.0.clone());
        }
        ObsCollectorObserver {
            collector: self.0.clone(),
            key: Some(key),
        }
    }

    pub fn as_dyn(&self) -> DynObs<C::Output> {
        self.obs().into_dyn()
    }
    pub fn obs(&self) -> Obs<ObsCollector<C>> {
        Obs(self.clone())
    }
}
impl<C: Collect> Observable for ObsCollector<C> {
    type Item = C::Output;
    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        bc: &mut BindContext,
    ) -> U {
        f(&self.0.clone().get(bc), bc)
    }
}
impl<C> Clone for ObsCollector<C> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<C: Collect> ObsCollectorData<C> {
    pub fn get(self: Rc<Self>, bc: &mut BindContext) -> C::Output {
        let value = self.collector.borrow().collect();
        bc.bind(self);
        value
    }
}
impl<C: Collect> DynamicObservableInner for ObsCollectorData<C> {
    type Item = C::Output;
    fn dyn_with(
        self: Rc<Self>,
        f: &mut dyn FnMut(&Self::Item, &mut BindContext),
        bc: &mut BindContext,
    ) {
        f(&self.get(bc), bc)
    }
}
impl<C: 'static> BindSource for ObsCollectorData<C> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}

impl<C: Collect> Observer<C::Input> for ObsCollectorObserver<C> {
    fn next(&mut self, value: C::Input) {
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
impl<C: Collect> Drop for ObsCollectorObserver<C> {
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
impl<C: Collect> RawSink for ObsCollector<C> {
    type Item = C::Input;
    type Observer = ObsCollectorObserver<C>;
    fn connect(&self, value: C::Input) -> Self::Observer {
        let mut o = self.insert();
        o.next(value);
        o
    }
}

impl<C: Collect> IntoSink<C::Input> for ObsCollector<C> {
    type RawSink = Self;

    fn into_sink(self) -> Sink<Self::RawSink> {
        Sink(self)
    }
}
impl<C: Collect> IntoSink<C::Input> for &ObsCollector<C> {
    type RawSink = ObsCollector<C>;

    fn into_sink(self) -> Sink<Self::RawSink> {
        self.clone().into_sink()
    }
}
