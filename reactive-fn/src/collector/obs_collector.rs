use crate::{
    core::{Action, ActionContext, BindSource, ObsContext, SinkBindings, UpdateContext},
    observable::{Obs, ObsBuilder, Observable, ObservableBuilder, RcObservable},
};
use derive_ex::derive_ex;
use std::{cell::RefCell, rc::Rc};

const PARAM: usize = 0;

pub trait Collector: 'static {
    type Input;
    type Output;
    type Key;
    fn insert(&mut self) -> CollectModify<Self::Key>;
    fn remove(&mut self, key: Self::Key) -> CollectModify;
    fn set(&mut self, key: Self::Key, value: Self::Input) -> CollectModify<Self::Key>;
    fn collect(&self) -> Self::Output;
}

struct RawObsCollector<C> {
    value: RefCell<C>,
    sinks: RefCell<SinkBindings>,
}

impl<C: Collector> RawObsCollector<C> {
    fn new(collect: C) -> Self {
        Self {
            value: RefCell::new(collect),
            sinks: RefCell::new(SinkBindings::new()),
        }
    }

    fn notify(&self, ac: &mut ActionContext) {
        self.sinks.borrow_mut().notify(true, ac.uc());
    }

    fn watch(self: &Rc<Self>, oc: &mut ObsContext) {
        self.sinks.borrow_mut().watch(self.clone(), PARAM, oc);
    }

    fn output(self: &Rc<Self>, oc: &mut ObsContext) -> C::Output {
        let value = self.value.borrow().collect();
        self.watch(oc);
        value
    }
}
impl<C: Collector> RcObservable for RawObsCollector<C> {
    type Item = C::Output;

    fn rc_with<U>(
        self: &Rc<Self>,
        f: impl FnOnce(&Self::Item, &mut ObsContext) -> U,
        oc: &mut ObsContext,
    ) -> U {
        f(&self.output(oc), oc)
    }
}

impl<C: Collector> BindSource for RawObsCollector<C> {
    fn flush(self: Rc<Self>, _param: usize, _uc: &mut UpdateContext) -> bool {
        false
    }
    fn unbind(self: Rc<Self>, _param: usize, key: usize, _uc: &mut UpdateContext) {
        self.sinks.borrow_mut().unbind(key);
    }
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

#[derive_ex(Clone(bound()), Default(bound(C)))]
#[default(Self::new())]
pub struct ObsCollector<C: Collector>(Rc<RawObsCollector<C>>);

impl<C: Collector> ObsCollector<C> {
    pub fn new() -> Self
    where
        C: Default,
    {
        Self::from(C::default())
    }
    pub fn from(collect: C) -> Self {
        Self(Rc::new(RawObsCollector::new(collect)))
    }

    pub fn insert(&self, ac: &mut ActionContext) -> ObsCollectorEntry<C> {
        let m = self.0.value.borrow_mut().insert();
        if m.is_modified {
            self.0.notify(ac);
        }
        ObsCollectorEntry::new(self.0.clone(), m.key)
    }

    pub fn obs(&self) -> Obs<C::Output> {
        self.obs_builder().obs()
    }
    pub fn obs_builder(&self) -> ObsBuilder<impl ObservableBuilder<Item = C::Output>> {
        ObsBuilder::from_rc_rc(self.0.clone())
    }
}
impl<C: Collector> Observable for ObsCollector<C> {
    type Item = C::Output;
    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, oc: &mut ObsContext) -> U {
        f(&self.0.clone().output(oc), oc)
    }
}

struct RawObsCollectorEntry<C: Collector> {
    owner: Rc<RawObsCollector<C>>,
    key: Option<C::Key>,
}

impl<C: Collector> RawObsCollectorEntry<C> {
    fn try_set(&mut self, value: C::Input, ac: &mut ActionContext) {
        if let Some(key) = self.key.take() {
            let m = self.owner.value.borrow_mut().set(key, value);
            self.key = Some(m.key);
            if m.is_modified {
                self.owner.notify(ac);
            }
        }
    }
    fn try_remove(&mut self, ac: &mut ActionContext) {
        if let Some(key) = self.key.take() {
            if self.owner.value.borrow_mut().remove(key).is_modified {
                self.owner.notify(ac);
            }
        }
    }
    fn remove_lazy(&mut self) {
        let key = self.key.take();
        if key.is_some() {
            let mut e = Self {
                key,
                owner: self.owner.clone(),
            };
            Action::new(move |ac| e.try_remove(ac)).schedule();
        }
    }
}

pub struct ObsCollectorEntry<C: Collector>(RawObsCollectorEntry<C>);
impl<C: Collector> ObsCollectorEntry<C> {
    fn new(collector: Rc<RawObsCollector<C>>, key: C::Key) -> Self {
        Self(RawObsCollectorEntry {
            owner: collector,
            key: Some(key),
        })
    }

    pub fn set(&mut self, value: C::Input, ac: &mut ActionContext) {
        self.0.try_set(value, ac)
    }
    pub fn remove(mut self, ac: &mut ActionContext) {
        self.0.try_remove(ac)
    }
}

impl<C: Collector> Drop for ObsCollectorEntry<C> {
    fn drop(&mut self) {
        self.0.remove_lazy()
    }
}
