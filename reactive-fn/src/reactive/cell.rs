use crate::Runtime;
use std::cell::{Cell, RefCell};
use std::ops::{Deref, DerefMut};
use std::{any::Any, rc::Rc};

use super::*;

/// A `Cell` like type that implement `Observable`.
pub struct ReCell<T: Copy>(Rc<ReCellData<T>>);

struct ReCellData<T: Copy> {
    value: Cell<T>,
    sinks: BindSinks,
}
impl<T: Copy + 'static> ReCell<T> {
    pub fn new(value: T) -> Self {
        Self(Rc::new(ReCellData {
            value: Cell::new(value),
            sinks: BindSinks::new(),
        }))
    }

    pub fn get(&self, cx: &BindContext) -> T {
        self.0.get(cx)
    }
    pub fn get_direct(&self) -> T {
        self.0.value.get()
    }

    pub fn set_with(&self, value: T, scope: &NotifyScope) {
        self.0.value.set(value);
        self.0.sinks.notify(scope);
    }

    pub fn set(&self, value: T) {
        self.0.value.set(value);
        Runtime::notify_defer(self.0.clone());
    }

    pub fn re(&self) -> DynObs<T> {
        DynObs(DynObsData::DynSource(self.0.clone()))
    }
    pub fn ops(&self) -> ReOps<impl Observable<Item = T> + Clone> {
        ReOps(self.clone())
    }
    pub fn ops_ref(&self) -> ReRefOps<impl ObservableRef<Item = T> + Clone> {
        self.ops().as_ref()
    }
}
impl<T: Copy + 'static> ReCellData<T> {
    fn get(self: &Rc<Self>, cx: &BindContext) -> T {
        cx.bind(self.clone());
        self.value.get()
    }
}
impl<T: Copy + 'static> Observable for ReCell<T> {
    type Item = T;
    fn get(&self, cx: &BindContext) -> Self::Item {
        self.0.get(cx)
    }
}

impl<T: Copy + 'static> DynamicObservableSource for ReCellData<T> {
    type Item = T;
    fn dyn_get(self: Rc<Self>, cx: &BindContext) -> Self::Item {
        self.get(cx)
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRefSource<Item = Self::Item>> {
        self
    }
}
impl<T: Copy + 'static> DynamicObservableRefSource for ReCellData<T> {
    type Item = T;
    fn dyn_with(self: Rc<Self>, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
        f(&self.get(cx), cx)
    }
}

impl<T: Copy + 'static> BindSource for ReCellData<T> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
impl<T: Copy> Clone for ReCell<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T: Copy + std::fmt::Debug> std::fmt::Debug for ReCell<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        std::fmt::Debug::fmt(&self.0.value, f)
    }
}

/// A `RefCell` like type that implement `ObservableRef`.
pub struct ReRefCell<T>(Rc<ReRefCellData<T>>);
struct ReRefCellData<T> {
    value: RefCell<T>,
    sinks: BindSinks,
}
impl<T: 'static> ReRefCell<T> {
    pub fn new(value: T) -> Self {
        Self(Rc::new(ReRefCellData {
            value: RefCell::new(value),
            sinks: BindSinks::new(),
        }))
    }
    pub fn set_with(&self, value: T, scope: &NotifyScope) {
        *self.0.value.borrow_mut() = value;
        self.0.sinks.notify(scope);
    }
    pub fn set(&self, value: T) {
        NotifyScope::with(|scope| self.set_with(value, scope));
    }

    pub fn borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, T> {
        self.0.borrow(cx)
    }
    pub fn borrow_direct(&self) -> Ref<T> {
        self.0.value.borrow()
    }
    pub fn borrow_mut<'a>(&'a self, cx: &'a NotifyScope) -> RefMut<'a, T> {
        RefMut {
            cx,
            b: self.0.value.borrow_mut(),
            sinks: &self.0.sinks,
            modified: false,
        }
    }
    pub fn re_borrow(&self) -> ReBorrow<T> {
        ReBorrow::from_dyn_source(self.0.clone())
    }
    pub fn re_ref(&self) -> ReRef<T> {
        self.re_borrow().as_ref()
    }
    pub fn ops(&self) -> ReBorrowOps<impl ObservableBorrow<Item = T> + Clone> {
        ReBorrowOps(self.clone())
    }
    pub fn ops_ref(&self) -> ReRefOps<impl ObservableRef<Item = T> + Clone> {
        self.ops().as_ref()
    }
}
impl<T: 'static> ReRefCellData<T> {
    pub fn borrow<'a>(self: &'a Rc<Self>, cx: &BindContext<'a>) -> Ref<'a, T> {
        cx.bind(self.clone());
        self.value.borrow()
    }
}

impl<T: 'static> ObservableBorrow for ReRefCell<T> {
    type Item = T;
    fn borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.0.borrow(cx)
    }
}

impl<T: 'static> DynamicObservableBorrowSource for ReRefCellData<T> {
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
impl<T: 'static> DynamicObservableRefSource for ReRefCellData<T> {
    type Item = T;
    fn dyn_with(self: Rc<Self>, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
        f(&self.borrow(cx), cx)
    }
}

impl<T: 'static> BindSource for ReRefCellData<T> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
impl<T> Clone for ReRefCell<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T: std::fmt::Debug> std::fmt::Debug for ReRefCell<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        std::fmt::Debug::fmt(&self.0.value, f)
    }
}

/// A wrapper type for a mutably borrowed value from a `BindRefCell<T>`.
pub struct RefMut<'a, T> {
    cx: &'a NotifyScope,
    b: std::cell::RefMut<'a, T>,
    sinks: &'a BindSinks,
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
            self.sinks.notify(self.cx);
        }
    }
}
