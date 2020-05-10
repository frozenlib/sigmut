use std::any::Any;
use std::cell::{Cell, RefCell};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use crate::*;

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

    pub fn set(&self, value: T) {
        self.0.value.set(value);
        self.0.sinks.notify();
    }
}
impl<T: Copy + 'static> ReCellData<T> {
    fn get(self: &Rc<Self>, ctx: &mut ReactiveContext) -> T {
        ctx.bind(self.clone());
        self.value.get()
    }
}
impl<T: Copy + 'static> Reactive for ReCell<T> {
    type Item = T;

    fn get(&self, ctx: &mut ReactiveContext) -> Self::Item {
        self.0.get(ctx)
    }
}
impl<T: Copy + 'static> InnerRe for ReCellData<T> {
    type Item = T;
    fn dyn_get(self: Rc<Self>, ctx: &mut ReactiveContext) -> Self::Item {
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
    pub fn borrow_mut(&self) -> RefMut<T> {
        RefMut {
            b: self.0.value.borrow_mut(),
            sinks: &self.0.sinks,
            modified: false,
        }
    }
}
impl<T: 'static> ReRefCellData<T> {
    fn borrow<'a>(self: &'a Rc<Self>, ctx: &mut ReactiveContext) -> Ref<'a, T> {
        ctx.bind(self.clone());
        Ref::Cell(self.value.borrow())
    }
}
impl<T: 'static> ReactiveRef for ReRefCell<T> {
    type Item = T;

    fn borrow(&self, ctx: &mut ReactiveContext) -> Ref<Self::Item> {
        self.0.borrow(ctx)
    }
}
impl<T: 'static> InnerReRef for ReRefCellData<T> {
    type Item = T;
    fn dyn_borrow<'a>(
        &'a self,
        rc_self: &'a dyn Any,
        ctx: &mut ReactiveContext,
    ) -> Ref<'a, Self::Item> {
        Self::downcast(rc_self).borrow(ctx)
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
            self.sinks.notify();
        }
    }
}
