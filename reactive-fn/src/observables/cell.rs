use crate::Runtime;
use std::cell::{Cell, RefCell};
use std::ops::{Deref, DerefMut};
use std::{any::Any, rc::Rc};

use super::*;

/// A `Rc<Cell>` like type that implement [`Observable`].
pub struct ObsCell<T: Copy>(Rc<ObsCellData<T>>);

struct ObsCellData<T: Copy> {
    value: Cell<T>,
    sinks: BindSinks,
}
impl<T: Copy + 'static> ObsCell<T> {
    pub fn new(value: T) -> Self {
        Self(Rc::new(ObsCellData {
            value: Cell::new(value),
            sinks: BindSinks::new(),
        }))
    }

    pub fn get(&self, cx: &BindContext) -> T {
        self.0.get(cx)
    }
    pub fn get_head(&self) -> T {
        self.0.value.get()
    }
    pub fn set(&self, value: T) {
        self.0.value.set(value);
        Runtime::spawn_notify(self.0.clone());
    }
    pub fn set_if_ne(&self, value: T)
    where
        T: PartialEq,
    {
        if self.get_head() != value {
            self.set(value);
        }
    }

    pub fn as_dyn(&self) -> DynObs<T> {
        DynObs(DynObsData::DynSource(self.0.clone()))
    }
    pub fn as_dyn_ref(&self) -> DynObsRef<T> {
        self.as_dyn().as_ref()
    }
    pub fn obs(&self) -> Obs<impl Observable<Item = T> + Clone> {
        Obs(self.clone())
    }
    pub fn obs_ref(&self) -> ObsRef<impl ObservableRef<Item = T> + Clone> {
        self.obs().as_ref()
    }
}
impl<T: Copy + 'static> ObsCellData<T> {
    fn get(self: &Rc<Self>, cx: &BindContext) -> T {
        cx.bind(self.clone());
        self.value.get()
    }
}
impl<T: Copy + 'static> Observable for ObsCell<T> {
    type Item = T;
    fn get(&self, cx: &BindContext) -> Self::Item {
        self.0.get(cx)
    }

    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        self.as_dyn()
    }
}

impl<T: Copy + 'static> DynamicObservableSource for ObsCellData<T> {
    type Item = T;
    fn dyn_get(self: Rc<Self>, cx: &BindContext) -> Self::Item {
        self.get(cx)
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRefSource<Item = Self::Item>> {
        self
    }
}
impl<T: Copy + 'static> DynamicObservableRefSource for ObsCellData<T> {
    type Item = T;
    fn dyn_with(self: Rc<Self>, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
        f(&self.get(cx), cx)
    }
}

impl<T: Copy + 'static> BindSource for ObsCellData<T> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
impl<T: Copy> Clone for ObsCell<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T: Copy + std::fmt::Debug> std::fmt::Debug for ObsCell<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        std::fmt::Debug::fmt(&self.0.value, f)
    }
}
impl<T: Copy + 'static> Observer for ObsCell<T> {
    type Item = T;
    fn next(&mut self, value: T) {
        self.set(value)
    }
}

/// A `Rc<RefCell>` like type that implement [`ObservableRef`].
pub struct ObsRefCell<T>(Rc<ObsRefCellData<T>>);
struct ObsRefCellData<T> {
    value: RefCell<T>,
    sinks: BindSinks,
}
impl<T: 'static> ObsRefCell<T> {
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

    pub fn borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, T> {
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
    pub fn borrow<'a>(self: &'a Rc<Self>, cx: &BindContext<'a>) -> Ref<'a, T> {
        cx.bind(self.clone());
        self.value.borrow()
    }
}

impl<T: 'static> ObservableBorrow for ObsRefCell<T> {
    type Item = T;
    fn borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.0.borrow(cx)
    }

    fn into_dyn(self) -> DynObsBorrow<Self::Item>
    where
        Self: Sized,
    {
        self.as_dyn()
    }
}

impl<T: 'static> DynamicObservableBorrowSource for ObsRefCellData<T> {
    type Item = T;

    fn dyn_borrow<'a>(
        &'a self,
        rc_self: &Rc<dyn DynamicObservableBorrowSource<Item = Self::Item>>,
        cx: &BindContext<'a>,
    ) -> Ref<'a, Self::Item> {
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
    fn dyn_with(self: Rc<Self>, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
        f(&self.borrow(cx), cx)
    }
}

impl<T: 'static> BindSource for ObsRefCellData<T> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
impl<T> Clone for ObsRefCell<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T: std::fmt::Debug> std::fmt::Debug for ObsRefCell<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        std::fmt::Debug::fmt(&self.0.value, f)
    }
}

/// A wrapper type for a mutably borrowed value from a [`ObsRefCell`].
pub struct RefMut<'a, T: 'static> {
    b: std::cell::RefMut<'a, T>,
    s: Option<ObsRefCell<T>>,
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
