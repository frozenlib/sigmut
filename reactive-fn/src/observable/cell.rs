use super::{Obs, ObservableBuilder, RcObservable};
use crate::{
    core::{
        schedule_notify_lazy, ActionContext, BindSink, BindSource, ObsContext, SinkBindings,
        UpdateContext,
    },
    ObsBuilder, Observable,
};
use derive_ex::derive_ex;
use reactive_fn_macros::ObservableFmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    cell::{Ref, RefCell, RefMut},
    ops::{Deref, DerefMut},
    rc::Rc,
};

const SLOT: usize = 0;

struct RawObsCell<T> {
    value: RefCell<T>,
    sinks: RefCell<SinkBindings>,
}

impl<T: 'static> RawObsCell<T> {
    fn watch(self: &Rc<Self>, oc: &mut ObsContext) {
        self.sinks.borrow_mut().watch(self.clone(), SLOT, oc);
    }
}

#[derive_ex(Clone(bound()))]
#[derive(ObservableFmt)]
#[observable_fmt(self_crate, bound(T))]
pub struct ObsCell<T: 'static>(Rc<RawObsCell<T>>);

impl<T: 'static> ObsCell<T> {
    #[inline]
    pub fn new(value: T) -> Self {
        Self(Rc::new(RawObsCell {
            value: RefCell::new(value),
            sinks: RefCell::new(SinkBindings::new()),
        }))
    }

    #[inline]
    pub fn obs(&self) -> Obs<T> {
        self.obs_builder().obs()
    }

    #[inline]
    pub fn obs_builder(&self) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder::from_rc_rc(self.0.clone())
    }

    #[inline]
    pub fn get(&self, oc: &mut ObsContext) -> T
    where
        T: Clone,
    {
        self.0.watch(oc);
        self.0.value.borrow().clone()
    }

    #[inline]
    pub fn set(&self, value: T, ac: &mut ActionContext) {
        self.0.sinks.borrow_mut().notify(true, ac.uc());
        *self.0.value.borrow_mut() = value;
    }

    #[inline]
    pub fn borrow<'a, 'b: 'a>(&'a self, oc: &mut ObsContext<'b>) -> Ref<'a, T> {
        self.0.watch(oc);
        self.0.value.borrow()
    }

    #[inline]
    pub fn borrow_mut<'a, 'b: 'a>(&'a self, _ac: &mut ActionContext<'b>) -> ObsCellRefMut<'a, T> {
        let value = self.0.value.borrow_mut();
        ObsCellRefMut {
            value,
            node: &self.0,
        }
    }
}

impl<T: Default> Default for ObsCell<T> {
    #[inline]
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: Serialize> Serialize for ObsCell<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.value.borrow().serialize(serializer)
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for ObsCell<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(T::deserialize(deserializer)?))
    }
}

impl<T: 'static> RcObservable for RawObsCell<T> {
    type Item = T;

    #[inline]
    fn rc_with<U>(
        self: &Rc<Self>,
        f: impl FnOnce(&Self::Item, &mut ObsContext) -> U,
        oc: &mut ObsContext,
    ) -> U {
        self.watch(oc);
        f(&self.value.borrow(), oc)
    }
}
impl<T: 'static> Observable for ObsCell<T> {
    type Item = T;

    #[inline]
    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, oc: &mut ObsContext) -> U {
        self.0.with(f, oc)
    }
}

impl<T: 'static> BindSource for RawObsCell<T> {
    fn flush(self: Rc<Self>, _slot: usize, _uc: &mut UpdateContext) -> bool {
        false
    }
    fn unbind(self: Rc<Self>, _slot: usize, key: usize, _uc: &mut UpdateContext) {
        self.sinks.borrow_mut().unbind(key);
    }
}
impl<T: 'static> BindSink for RawObsCell<T> {
    fn notify(self: Rc<Self>, _slot: usize, is_modified: bool, uc: &mut UpdateContext) {
        self.sinks.borrow_mut().notify(is_modified, uc);
    }
}

pub struct ObsCellRefMut<'a, T: 'static> {
    value: RefMut<'a, T>,
    node: &'a Rc<RawObsCell<T>>,
}

impl<'a, T> Deref for ObsCellRefMut<'a, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
impl<'a, T> DerefMut for ObsCellRefMut<'a, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}
impl<'a, T: 'static> Drop for ObsCellRefMut<'a, T> {
    fn drop(&mut self) {
        let node = Rc::downgrade(self.node);
        schedule_notify_lazy(node, SLOT)
    }
}
