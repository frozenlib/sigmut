use std::cell::{Cell, RefCell};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use crate::*;

pub struct BindCell<T: Copy>(Rc<BindCellData<T>>);

struct BindCellData<T: Copy> {
    value: Cell<T>,
    sinks: BindSinks,
}
impl<T: Copy + 'static> BindCell<T> {
    pub fn new(value: T) -> Self {
        Self(Rc::new(BindCellData {
            value: Cell::new(value),
            sinks: BindSinks::new(),
        }))
    }

    pub fn set(&self, value: T) {
        self.0.value.set(value);
        self.0.sinks.notify();
    }

    pub fn ext(&self) -> BindExt<Self> {
        self.clone().into_ext()
    }
}
impl<T: Copy + 'static> Bind for BindCell<T> {
    type Item = T;

    fn bind(&self, ctx: &mut BindContext) -> Self::Item {
        ctx.bind(self.0.clone());
        self.0.value.get()
    }
}
impl<T: Copy + 'static> BindSource for BindCellData<T> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
impl<T: Copy> Clone for BindCell<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T: Copy + std::fmt::Debug> std::fmt::Debug for BindCell<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        std::fmt::Debug::fmt(&self.0.value, f)
    }
}

pub struct RefBindCell<T>(Rc<RefBindCellData<T>>);
struct RefBindCellData<T> {
    value: RefCell<T>,
    sinks: BindSinks,
}
impl<T: 'static> RefBindCell<T> {
    pub fn borrow_mut(&self) -> RefMut<T> {
        RefMut {
            b: self.0.value.borrow_mut(),
            sinks: &self.0.sinks,
            modified: false,
        }
    }
    pub fn ext(&self) -> RefBindExt<Self> {
        self.clone().into_ext()
    }
}
impl<T: 'static> RefBind for RefBindCell<T> {
    type Item = T;

    fn bind(&self, ctx: &mut BindContext) -> Ref<Self::Item> {
        ctx.bind(self.0.clone());
        Ref::Cell(self.0.value.borrow())
    }
}
impl<T: 'static> BindSource for RefBindCellData<T> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
impl<T> Clone for RefBindCell<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T: std::fmt::Debug> std::fmt::Debug for RefBindCell<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        std::fmt::Debug::fmt(&self.0.value, f)
    }
}

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
