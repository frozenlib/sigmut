use crate::*;

use std::ops::Deref;
use std::rc::{Rc, Weak};

/// The context of `Reactive::bind` and `ReactiveRef::bind`.
pub struct ReactiveContext<'a> {
    sink: Weak<dyn BindSink>,
    bindings: &'a mut Vec<Binding>,
}
impl<'a> ReactiveContext<'a> {
    pub fn new(sink: &Rc<impl BindSink + 'static>, bindings: &'a mut Vec<Binding>) -> Self {
        debug_assert!(bindings.is_empty());
        Self {
            sink: Rc::downgrade(sink) as Weak<dyn BindSink>,
            bindings,
        }
    }
    pub fn bind(&mut self, src: Rc<impl BindSource>) {
        self.bindings.push(src.bind(self.sink.clone()));
    }
}

pub trait Reactive: 'static {
    type Item;

    fn get(&self, ctx: &mut ReactiveContext) -> Self::Item;

    fn into_ext(self) -> BindExt<Self>
    where
        Self: Sized,
    {
        BindExt(self)
    }

    fn into_rc(self) -> RcRe<Self::Item>
    where
        Self: Sized,
    {
        Rc::new(self)
    }
}

pub trait ReactiveRef: 'static {
    type Item;

    fn borrow(&self, ctx: &mut ReactiveContext) -> Ref<Self::Item>;

    fn into_ext(self) -> RefBindExt<Self>
    where
        Self: Sized,
    {
        RefBindExt(self)
    }
    fn into_rc(self) -> RcReRef<Self::Item>
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
