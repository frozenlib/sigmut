use super::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    cell::{Ref, RefCell},
    ops::{Deref, DerefMut},
    rc::Rc,
};

/// A `Rc<RefCell>` like type that implement [`Observable`].
pub struct ObsCell<T: 'static>(Rc<ObsRefCellData<T>>);
struct ObsRefCellData<T> {
    value: RefCell<T>,
    sinks: BindSinks,
}
impl<T: 'static> ObsCell<T> {
    pub fn new(value: T) -> Self {
        Self(Rc::new(ObsRefCellData {
            value: RefCell::new(value),
            sinks: BindSinks::new(),
        }))
    }
    pub fn set(&self, value: T) {
        *self.0.value.borrow_mut() = value;
        Runtime::spawn_notify(self.0.clone());
    }
    pub fn set_dedup(&self, value: T)
    where
        T: PartialEq,
    {
        let mut b = self.0.value.borrow_mut();
        if *b != value {
            *b = value;
            Runtime::spawn_notify(self.0.clone());
        }
    }
    pub fn get(&self, bc: &mut BindContext) -> T
    where
        T: Clone,
    {
        self.0.borrow(bc).clone()
    }
    pub fn get_head(&self) -> T
    where
        T: Clone,
    {
        self.0.value.borrow().clone()
    }

    pub fn with<U>(&self, f: impl FnOnce(&T, &mut BindContext) -> U, bc: &mut BindContext) -> U {
        f(&self.borrow(bc), bc)
    }

    pub fn borrow(&self, bc: &mut BindContext) -> Ref<T> {
        self.0.borrow(bc)
    }
    pub fn borrow_head(&self) -> Ref<T> {
        self.0.value.borrow()
    }
    pub fn borrow_mut(&self) -> RefMut<T> {
        RefMut {
            b: self.0.value.borrow_mut(),
            s: Some(self.clone()),
            modified: false,
        }
    }
    pub fn borrow_mut_dedup(&self) -> RefMutDedup<T>
    where
        T: PartialEq + Clone,
    {
        RefMutDedup {
            b: self.0.value.borrow_mut(),
            s: Some(self.clone()),
            old: None,
        }
    }

    pub fn as_dyn(&self) -> DynObs<T> {
        self.obs().into_dyn()
    }
    pub fn obs(&self) -> Obs<ObsCell<T>> {
        Obs(self.clone())
    }
}
impl<T: 'static> ObsRefCellData<T> {
    pub fn borrow<'a>(self: &'a Rc<Self>, bc: &mut BindContext) -> Ref<'a, T> {
        bc.bind(self.clone());
        self.value.borrow()
    }
}
impl<T: 'static> Observable for ObsCell<T> {
    type Item = T;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        bc: &mut BindContext,
    ) -> U {
        self.with(f, bc)
    }
    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        DynObs::new_dyn_inner(self.0)
    }
}

impl<T: 'static> DynamicObservableInner for ObsRefCellData<T> {
    type Item = T;
    fn dyn_with(
        self: Rc<Self>,
        f: &mut dyn FnMut(&Self::Item, &mut BindContext),
        bc: &mut BindContext,
    ) {
        f(&self.borrow(bc), bc)
    }
}

impl<T: 'static> BindSource for ObsRefCellData<T> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
impl<T> Clone for ObsCell<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T: Serialize> Serialize for ObsCell<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.deref().serialize(serializer)
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

/// A wrapper type for a mutably borrowed value from a [`ObsCell`].
pub struct RefMut<'a, T: 'static> {
    b: std::cell::RefMut<'a, T>,
    s: Option<ObsCell<T>>,
    modified: bool,
}

impl<T> Deref for RefMut<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.b
    }
}
impl<T> DerefMut for RefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.modified = true;
        &mut self.b
    }
}
impl<T> Drop for RefMut<'_, T> {
    fn drop(&mut self) {
        if self.modified {
            Runtime::spawn_notify(self.s.take().unwrap().0);
        }
    }
}
pub struct RefMutDedup<'a, T: 'static + PartialEq> {
    b: std::cell::RefMut<'a, T>,
    s: Option<ObsCell<T>>,
    old: Option<T>,
}
impl<T: 'static + PartialEq> Deref for RefMutDedup<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.b
    }
}
impl<T: 'static + PartialEq + Clone> DerefMut for RefMutDedup<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        if self.old.is_none() {
            self.old = Some(self.b.clone());
        }
        &mut self.b
    }
}
impl<T: 'static + PartialEq> Drop for RefMutDedup<'_, T> {
    fn drop(&mut self) {
        if let Some(old) = &self.old {
            if old != &*self.b {
                Runtime::spawn_notify(self.s.take().unwrap().0);
            }
        }
    }
}
