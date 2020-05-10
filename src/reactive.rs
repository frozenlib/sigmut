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

    fn into_re(self) -> Re<Self::Item>
    where
        Self: Sized,
    {
        Re::from_inner(self)
    }
}

pub trait ReactiveRef: 'static {
    type Item;

    fn borrow(&self, ctx: &mut ReactiveContext) -> Ref<Self::Item>;

    fn into_re_ref(self) -> ReRef<Self::Item>
    where
        Self: Sized,
    {
        ReRef::from_inner(self)
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

pub fn from_get<T>(get: impl Fn(&mut ReactiveContext) -> T + 'static) -> impl Reactive<Item = T> {
    struct FnBind<F>(F);
    impl<F: Fn(&mut ReactiveContext) -> T + 'static, T> Reactive for FnBind<F> {
        type Item = T;
        fn get(&self, ctx: &mut ReactiveContext) -> Self::Item {
            (self.0)(ctx)
        }
    }
    FnBind(get)
}

pub fn from_borrow<T, F, U>(this: T, borrow: F) -> impl ReactiveRef<Item = U>
where
    T: 'static,
    for<'a> F: Fn(&'a T, &mut ReactiveContext) -> Ref<'a, U> + 'static,
    U: 'static,
{
    struct FnRefBind<T, F> {
        this: T,
        borrow: F,
    }
    impl<T, F, U> ReactiveRef for FnRefBind<T, F>
    where
        T: 'static,
        for<'a> F: Fn(&'a T, &mut ReactiveContext) -> Ref<'a, U> + 'static,
        U: 'static,
    {
        type Item = U;
        fn borrow(&self, ctx: &mut ReactiveContext) -> Ref<U> {
            (self.borrow)(&self.this, ctx)
        }
    }

    FnRefBind { this, borrow }
}
