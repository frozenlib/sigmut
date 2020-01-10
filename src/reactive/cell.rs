use std::cell;
use std::cell::RefCell;
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use super::*;
use crate::binding::*;

#[derive(Clone)]
pub struct ReCell<T>(Rc<ReCellData<T>>);
struct ReCellData<T> {
    value: RefCell<T>,
    sinks: BindSinks,
}

impl<T> ReCell<T> {
    pub fn new(value: T) -> Self {
        Self(Rc::new(ReCellData {
            value: RefCell::new(value),
            sinks: BindSinks::new(),
        }))
    }
    pub fn set(&self, value: T) {
        *self.borrow_mut() = value;
    }
    pub fn borrow_mut(&self) -> RefMut<T> {
        RefMut {
            b: ManuallyDrop::new(self.0.value.borrow_mut()),
            sinks: &self.0.sinks,
            modified: false,
        }
    }
    pub fn lock(&self) -> LockGuard<T> {
        self.0.sinks.lock();
        LockGuard(self)
    }
}

impl<T: 'static> ReRef for ReCell<T> {
    type Item = T;
    fn borrow(&self, ctx: &mut BindContext) -> Ref<T> {
        ctx.bind(self.0.clone());
        Ref::RefCell(self.0.value.borrow())
    }
}

impl<T> BindSource for ReCellData<T> {
    fn bind_sinks(&self) -> &BindSinks {
        &self.sinks
    }
}

pub struct LockGuard<'a, T>(&'a ReCell<T>);
impl<'a, T> Deref for LockGuard<'a, T> {
    type Target = ReCell<T>;
    fn deref(&self) -> &ReCell<T> {
        &self.0
    }
}
impl<'a, T> Drop for LockGuard<'a, T> {
    fn drop(&mut self) {
        (self.0).0.sinks.unlock(false);
    }
}

pub struct RefMut<'a, T> {
    b: ManuallyDrop<cell::RefMut<'a, T>>,
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
        unsafe {
            ManuallyDrop::drop(&mut self.b);
        }
        if self.modified {
            self.sinks.lock();
            self.sinks.unlock(true);
        }
    }
}
