use super::*;
use std::{
    any::Any,
    cell::{Ref, RefCell},
    ops::{Deref, DerefMut},
    rc::Rc,
};

/// A `Rc<RefCell>` like type that implement [`ObservableRef`].
pub struct ObsCell<T>(Rc<ObsRefCellData<T>>);
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
        T: Copy,
    {
        *self.0.borrow(cx)
    }
    pub fn get_head(&self) -> T
    where
        T: Copy,
    {
        *self.0.value.borrow()
    }

    pub fn borrow(&self, cx: &mut BindContext) -> Ref<T> {
        self.0.borrow(cx)
    }
    pub fn borrow_head(&self) -> Ref<T> {
        self.0.value.borrow()
    }
    pub fn borrow_mut<'a>(&'a self) -> RefMut<'a, T> {
        RefMut {
            b: self.0.value.borrow_mut(),
            s: Some(self.clone()),
            modified: false,
        }
    }
    pub fn as_dyn(&self) -> DynObsBorrow<T> {
        DynObsBorrow::from_dyn_source(self.0.clone())
    }
    pub fn as_dyn_ref(&self) -> DynObsRef<T> {
        self.as_dyn().as_ref()
    }
    pub fn obs(&self) -> ObsBorrow<impl ObservableBorrow<Item = T> + Clone> {
        ObsBorrow(self.clone())
    }
    pub fn obs_ref(&self) -> ObsRef<impl ObservableRef<Item = T> + Clone> {
        self.obs().as_ref()
    }
}
impl<T: 'static> ObsRefCellData<T> {
    pub fn borrow<'a>(self: &'a Rc<Self>, cx: &mut BindContext) -> Ref<'a, T> {
        cx.bind(self.clone());
        self.value.borrow()
    }
}

impl<T: 'static> ObservableBorrow for ObsCell<T> {
    type Item = T;
    fn borrow(&self, cx: &mut BindContext) -> Ref<Self::Item> {
        self.0.borrow(cx)
    }

    fn into_dyn_obs_borrow(self) -> DynObsBorrow<Self::Item>
    where
        Self: Sized,
    {
        self.as_dyn()
    }
}
impl<T: 'static> ObservableRef for ObsCell<T> {
    type Item = T;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        self.obs().as_ref().with(f, cx)
    }
    fn into_dyn_obs_ref(self) -> DynObsRef<Self::Item>
    where
        Self: Sized,
    {
        self.as_dyn().as_ref()
    }
}

impl<T: 'static> DynamicObservableBorrowSource for ObsRefCellData<T> {
    type Item = T;

    fn dyn_borrow(
        &self,
        rc_self: &Rc<dyn DynamicObservableBorrowSource<Item = Self::Item>>,
        cx: &mut BindContext,
    ) -> Ref<Self::Item> {
        cx.bind(Self::downcast(rc_self));
        self.value.borrow()
    }

    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRefSource<Item = Self::Item>> {
        self
    }
}
impl<T: 'static> DynamicObservableRefSource for ObsRefCellData<T> {
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
impl<T: std::fmt::Debug> std::fmt::Debug for ObsCell<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        std::fmt::Debug::fmt(&self.0.value, f)
    }
}

/// A wrapper type for a mutably borrowed value from a [`ObsCell`].
pub struct RefMut<'a, T: 'static> {
    b: std::cell::RefMut<'a, T>,
    s: Option<ObsCell<T>>,
    modified: bool,
}

impl<'a, T> Deref for RefMut<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.b
    }
}
impl<'a, T> DerefMut for RefMut<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.modified = true;
        &mut self.b
    }
}
impl<'a, T> Drop for RefMut<'a, T> {
    fn drop(&mut self) {
        if self.modified {
            Runtime::spawn_notify(self.s.take().unwrap().0);
        }
    }
}
