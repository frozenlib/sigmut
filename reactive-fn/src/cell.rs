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
    pub fn set_if_ne(&self, value: T)
    where
        T: PartialEq,
    {
        let mut b = self.0.value.borrow_mut();
        if *b != value {
            *b = value;
            Runtime::spawn_notify(self.0.clone());
        }
    }
    pub fn get(&self, cx: &mut BindContext) -> T
    where
        T: Clone,
    {
        self.0.borrow(cx).clone()
    }
    pub fn get_head(&self) -> T
    where
        T: Clone,
    {
        self.0.value.borrow().clone()
    }

    pub fn with<U>(&self, f: impl FnOnce(&T, &mut BindContext) -> U, cx: &mut BindContext) -> U {
        f(&self.borrow(cx), cx)
    }

    pub fn borrow(&self, cx: &mut BindContext) -> Ref<T> {
        self.0.borrow(cx)
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
    pub fn as_dyn(&self) -> DynObs<T> {
        self.obs().into_dyn()
    }
    pub fn obs(&self) -> Obs<ObsCell<T>> {
        Obs(self.clone())
    }
}
impl<T: 'static> ObsRefCellData<T> {
    pub fn borrow<'a>(self: &'a Rc<Self>, cx: &mut BindContext) -> Ref<'a, T> {
        cx.bind(self.clone());
        self.value.borrow()
    }
}
impl<T: 'static> Observable for ObsCell<T> {
    type Item = T;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        self.with(f, cx)
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
        cx: &mut BindContext,
    ) {
        f(&self.borrow(cx), cx)
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
