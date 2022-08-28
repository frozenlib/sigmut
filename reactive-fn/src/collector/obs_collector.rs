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
    fn insert(&mut self) -> CollectModify<Self::Key>;
    fn remove(&mut self, key: Self::Key) -> CollectModify;
    fn set(&mut self, key: Self::Key, value: Self::Input) -> CollectModify<Self::Key>;
    fn collect(&self) -> Self::Output;
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub struct CollectModify<K = ()> {
    pub key: K,
    pub is_modified: bool,
}

impl CollectModify<()> {
    pub fn from_is_modified(is_modified: bool) -> Self {
        Self {
            key: (),
            is_modified,
        }
    }
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
        let m = self.0.collector.borrow_mut().insert();
        if m.is_modified {
            schedule_notify(&self.0);
        }
        ObsCollectorObserver {
            collector: self.0.clone(),
            key: Some(m.key),
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

    fn dyn_with<'a>(
        self: Rc<Self>,
        oc: ObserverContext<'a, '_, '_, Self::Item>,
    ) -> ObserverResult<'a> {
        let value = self.get(oc.bc);
        oc.ret(&value)
    }
}
impl<C: 'static> BindSource for ObsCollectorData<C> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}

impl<C: Collect> Observer<C::Input> for ObsCollectorObserver<C> {
    fn next(&mut self, value: C::Input) {
        let m = self
            .collector
            .collector
            .borrow_mut()
            .set(self.key.take().unwrap(), value);
        self.key = Some(m.key);
        if m.is_modified {
            schedule_notify(&self.collector);
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
            .is_modified
        {
            schedule_notify(&self.collector);
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
