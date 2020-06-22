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

    pub fn get(&self, ctx: &BindContext) -> T {
        self.0.get(ctx)
    }

    pub fn set(&self, value: T, ctx: &NotifyContext) {
        self.0.value.set(value);
        self.0.sinks.notify(ctx);
    }

    pub fn set_and_update(&self, value: T) {
        self.0.value.set(value);
        NotifyContext::update(&self.0);
    }

    pub fn re(&self) -> Re<T> {
        Re(ReData::DynSource(self.0.clone()))
    }
    pub fn ops(&self) -> ReOps<impl Reactive<Item = T> + Clone> {
        ReOps(self.clone())
    }
    pub fn ops_ref(&self) -> ReRefOps<impl ReactiveRef<Item = T> + Clone> {
        self.ops().as_ref()
    }
}
impl<T: Copy + 'static> ReCellData<T> {
    fn get(self: &Rc<Self>, ctx: &BindContext) -> T {
        ctx.bind(self.clone());
        self.value.get()
    }
}
impl<T: Copy + 'static> Reactive for ReCell<T> {
    type Item = T;
    fn get(&self, ctx: &BindContext) -> Self::Item {
        self.0.get(ctx)
    }
}

impl<T: Copy + 'static> DynamicReactiveSource for ReCellData<T> {
    type Item = T;
    fn dyn_get(self: Rc<Self>, ctx: &BindContext) -> Self::Item {
        self.get(ctx)
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicReactiveRefSource<Item = Self::Item>> {
        self
    }
}
impl<T: Copy + 'static> DynamicReactiveRefSource for ReCellData<T> {
    type Item = T;
    fn dyn_with(self: Rc<Self>, ctx: &BindContext, f: &mut dyn FnMut(&BindContext, &Self::Item)) {
        f(ctx, &self.get(ctx))
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
        NotifyContext::with(|ctx| self.set(value, ctx));
    }

    pub fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, T> {
        self.0.borrow(ctx)
    }
    pub fn borrow_mut<'a>(&'a self, ctx: &'a NotifyContext) -> RefMut<'a, T> {
        RefMut {
            ctx,
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
    pub fn ops(&self) -> ReBorrowOps<impl ReactiveBorrow<Item = T> + Clone> {
        ReBorrowOps(self.clone())
    }
    pub fn ops_ref(&self) -> ReRefOps<impl ReactiveRef<Item = T> + Clone> {
        self.ops().as_ref()
    }
}
impl<T: 'static> ReRefCellData<T> {
    pub fn borrow<'a>(self: &'a Rc<Self>, ctx: &BindContext<'a>) -> Ref<'a, T> {
        ctx.bind(self.clone());
        self.value.borrow()
    }
}

impl<T: 'static> ReactiveBorrow for ReRefCell<T> {
    type Item = T;
    fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.0.borrow(ctx)
    }
}

impl<T: 'static> DynamicReactiveBorrowSource for ReRefCellData<T> {
    type Item = T;

    fn dyn_borrow(
        &self,
        rc_self: &Rc<dyn DynamicReactiveBorrowSource<Item = Self::Item>>,
        ctx: &BindContext,
    ) -> Ref<Self::Item> {
        ctx.bind(Self::downcast(rc_self));
        self.value.borrow()
    }

    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicReactiveRefSource<Item = Self::Item>> {
        self
    }
}
impl<T: 'static> DynamicReactiveRefSource for ReRefCellData<T> {
    type Item = T;
    fn dyn_with(self: Rc<Self>, ctx: &BindContext, f: &mut dyn FnMut(&BindContext, &Self::Item)) {
        f(ctx, &self.borrow(ctx))
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
