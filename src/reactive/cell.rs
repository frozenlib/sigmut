use std::cell::{Cell, RefCell};
use std::ops::{Deref, DerefMut};
use std::{any::Any, rc::Rc};

use super::*;

/// A `Cell` like type that implement `Reactive`.
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

    pub fn get(&self, ctx: &mut BindContext) -> T {
        self.0.get(ctx)
    }

    pub fn set(&self, value: T, ctx: &NotifyContext) {
        self.0.value.set(value);
        self.0.sinks.notify(ctx);
    }

    pub fn set_and_update(&self, value: T) {
        self.0.value.set(value);
        self.0.sinks.notify_and_update();
    }

    pub fn to_re(&self) -> Re<T> {
        Re(ReData::DynSource(self.0.clone()))
    }
}
impl<T: Copy + 'static> ReCellData<T> {
    fn get(self: &Rc<Self>, ctx: &mut BindContext) -> T {
        ctx.bind(self.clone());
        self.value.get()
    }
}
impl<T: Copy + 'static> DynReSource for ReCellData<T> {
    type Item = T;
    fn dyn_get(self: Rc<Self>, ctx: &mut BindContext) -> Self::Item {
        self.get(ctx)
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

/// A `RefCell` like type that implement `ReactiveRef`.
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
    pub fn set(&self, value: T, ctx: &NotifyContext) {
        *self.0.value.borrow_mut() = value;
        self.0.sinks.notify(ctx);
    }
    pub fn set_and_update(&self, value: T) {
        *self.0.value.borrow_mut() = value;
        self.0.sinks.notify_and_update();
    }
    pub fn borrow_mut<'a>(&'a self, ctx: &'a NotifyContext) -> RefMut<'a, T> {
        RefMut {
            ctx,
            b: self.0.value.borrow_mut(),
            sinks: &self.0.sinks,
            modified: false,
        }
    }
    pub fn to_re_ref(&self) -> ReRef<T> {
        self.to_re_borrow().to_re_ref()
    }
    pub fn to_re_borrow(&self) -> ReBorrow<T> {
        ReBorrow(ReBorrowData::DynSource(self.0.clone()))
    }
}
impl<T: 'static> DynReBorrowSource for ReRefCellData<T> {
    type Item = T;

    fn dyn_borrow(
        &self,
        rc_self: &Rc<dyn DynReBorrowSource<Item = Self::Item>>,
        ctx: &mut BindContext,
    ) -> Ref<Self::Item> {
        ctx.bind(Self::downcast(rc_self));
        self.value.borrow()
    }

    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
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
    ctx: &'a NotifyContext,
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
            self.sinks.notify(self.ctx);
        }
    }
}
