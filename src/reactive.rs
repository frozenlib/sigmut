use crate::*;

use std::ops::Deref;
use std::rc::Rc;

pub trait Reactive: 'static {
    type Item;

    fn get(&self, ctx: &mut BindContext) -> Self::Item;

    fn into_ext(self) -> BindExt<Self>
    where
        Self: Sized,
    {
        BindExt(self)
    }

    fn into_rc(self) -> RcBind<Self::Item>
    where
        Self: Sized,
    {
        Rc::new(self)
    }
}

pub trait ReactiveRef: 'static {
    type Item;

    fn borrow(&self, ctx: &mut BindContext) -> Ref<Self::Item>;

    fn into_ext(self) -> RefBindExt<Self>
    where
        Self: Sized,
    {
        RefBindExt(self)
    }
    fn into_rc(self) -> RcRefBind<Self::Item>
    where
        Self: Sized,
    {
        Rc::new(self)
    }
}

/// A wrapper type for an immutably borrowed value from a `ReactiveRef`.
pub enum Ref<'a, T> {
    Native(&'a T),
    Cell(std::cell::Ref<'a, T>),
}
impl<'a, T> Ref<'a, T> {
    pub fn map<U>(this: Self, f: impl FnOnce(&T) -> &U) -> Ref<'a, U> {
        use Ref::*;
        match this {
            Native(x) => Native(f(x)),
            Cell(x) => Cell(std::cell::Ref::map(x, f)),
        }
    }
}
impl<'a, T> Deref for Ref<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        match self {
            Ref::Native(x) => x,
            Ref::Cell(x) => x,
        }
    }
}
