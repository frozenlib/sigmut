use std::cell::{RefCell, RefMut};
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use crate::*;

#[derive(Clone)]
pub struct ReCell<T>(Rc<ReCellData<T>>);
struct ReCellData<T> {
    value: RefCell<T>,
    sinks: BindSinks,
}

impl<T> ReCell<T> {
    pub fn set(&self, value: T) {
        *self.borrow_mut() = value;
    }
    pub fn borrow_mut(&self) -> ReCellRefMut<T> {
        ReCellRefMut {
            b: ManuallyDrop::new(self.0.value.borrow_mut()),
            sinks: &self.0.sinks,
            modified: false,
        }
    }
    pub fn lock(&self) -> ReCellLockGuard<T> {
        self.0.sinks.lock();
        ReCellLockGuard(self)
    }
}

impl<T: Clone + 'static> Re for ReCell<T> {
    type Item = T;
    fn get(&self, ctx: &mut ReContext) -> Self::Item {
        self.borrow(ctx).clone()
    }
}
impl<T: 'static> ReRef for ReCell<T> {
    type Item = T;
    fn borrow(&self, ctx: &mut ReContext) -> ReBorrow<T> {
        ctx.bind(self.0.clone());
        ReBorrow::RefCell(self.0.value.borrow())
    }
}

impl<T> BindSource for ReCellData<T> {
    fn bind_sinks(&self) -> &BindSinks {
        &self.sinks
    }
}

pub struct ReCellLockGuard<'a, T>(&'a ReCell<T>);
impl<'a, T> Deref for ReCellLockGuard<'a, T> {
    type Target = ReCell<T>;
    fn deref(&self) -> &ReCell<T> {
        &self.0
    }
}
impl<'a, T> Drop for ReCellLockGuard<'a, T> {
    fn drop(&mut self) {
        (self.0).0.sinks.unlock(false);
    }
}

pub struct ReCellRefMut<'a, T> {
    b: ManuallyDrop<RefMut<'a, T>>,
    sinks: &'a BindSinks,
    modified: bool,
}

impl<'a, T> Deref for ReCellRefMut<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.b
    }
}
impl<'a, T> DerefMut for ReCellRefMut<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.modified = true;
        &mut self.b
    }
}
impl<'a, T> Drop for ReCellRefMut<'a, T> {
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.b);
        }
        if self.modified {
            self.sinks.lock();
            self.sinks.unlock(true);
        }
    }
}
