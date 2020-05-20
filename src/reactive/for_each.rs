use crate::bind::*;
use crate::reactive::*;
use std::{cell::RefCell, rc::Rc};

pub struct ForEach<T: 'static, F> {
    source: Re<T>,
    state: RefCell<ForEachState<F>>,
}
pub struct ForEachRef<T: 'static + ?Sized, F> {
    source: ReRef<T>,
    state: RefCell<ForEachState<F>>,
}

struct ForEachState<F> {
    f: F,
    bindings: Bindings,
}

impl<T: 'static, F: FnMut(T) + 'static> ForEach<T, F> {
    pub fn new(source: Re<T>, f: F) -> Rc<Self> {
        let s = Rc::new(ForEach {
            source,
            state: RefCell::new(ForEachState {
                f,
                bindings: Bindings::new(),
            }),
        });
        s.next();
        s
    }

    fn next(self: &Rc<Self>) {
        let b = &mut *self.state.borrow_mut();
        let value = b.bindings.update_root(self, |ctx| self.source.get(ctx));
        (b.f)(value);
    }
}
impl<T: 'static, F: FnMut(T) + 'static> BindSink for ForEach<T, F> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        ctx.spawn(Rc::downgrade(&self))
    }
}
impl<T: 'static, F: FnMut(T) + 'static> Task for ForEach<T, F> {
    fn run(self: Rc<Self>) {
        self.next();
    }
}

impl<T: 'static + ?Sized, F: FnMut(&T) + 'static> ForEachRef<T, F> {
    pub fn new(source: ReRef<T>, f: F) -> Rc<Self> {
        let s = Rc::new(ForEachRef {
            source,
            state: RefCell::new(ForEachState {
                f,
                bindings: Bindings::new(),
            }),
        });
        s.next();
        s
    }

    fn next(self: &Rc<Self>) {
        let b = &mut *self.state.borrow_mut();
        let f = &mut b.f;
        b.bindings
            .update_root(self, |ctx| self.source.with(ctx, |x| f(x)))
    }
}
impl<T: 'static + ?Sized, F: FnMut(&T) + 'static> BindSink for ForEachRef<T, F> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        self.state.borrow_mut().bindings.clear();
        ctx.spawn(Rc::downgrade(&self))
    }
}
impl<T: 'static + ?Sized, F: FnMut(&T) + 'static> Task for ForEachRef<T, F> {
    fn run(self: Rc<Self>) {
        self.next();
    }
}

pub struct ForEachBy<T, U, A, D>
where
    T: 'static,
    U: 'static,
    A: FnMut(T) -> U + 'static,
    D: FnMut(U) + 'static,
{
    source: Re<T>,
    state: RefCell<ForEachByState<U, A, D>>,
}

struct ForEachByState<U, A, D> {
    attach: A,
    detach: D,
    value: Option<U>,
    bindings: Bindings,
}

impl<T, U, A, D> ForEachBy<T, U, A, D>
where
    A: FnMut(T) -> U + 'static,
    D: FnMut(U) + 'static,
{
    pub fn new(source: Re<T>, attach: A, detach: D) -> Rc<Self> {
        let s = Rc::new(ForEachBy {
            source,
            state: RefCell::new(ForEachByState {
                attach,
                detach,
                value: None,
                bindings: Bindings::new(),
            }),
        });
        s.next();
        s
    }

    fn next(self: &Rc<Self>) {
        let mut b = &mut *self.state.borrow_mut();
        let attach = &mut b.attach;
        b.value = b
            .bindings
            .update_root(self, |ctx| Some(attach(self.source.get(ctx))));
    }
}
impl<U, A, D> ForEachByState<U, A, D>
where
    D: FnMut(U) + 'static,
{
    fn detach_value(&mut self) {
        if let Some(value) = self.value.take() {
            (self.detach)(value);
        }
    }
}

impl<T, U, A, D> BindSink for ForEachBy<T, U, A, D>
where
    A: FnMut(T) -> U + 'static,
    D: FnMut(U) + 'static,
{
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        let b = &mut *self.state.borrow_mut();
        b.bindings.clear();
        b.detach_value();
        drop(b);
        ctx.spawn(Rc::downgrade(&self))
    }
}
impl<T, U, A, D> Task for ForEachBy<T, U, A, D>
where
    A: FnMut(T) -> U + 'static,
    D: FnMut(U) + 'static,
{
    fn run(self: Rc<Self>) {
        self.next();
    }
}
impl<T, U, A, D> Drop for ForEachBy<T, U, A, D>
where
    A: FnMut(T) -> U + 'static,
    D: FnMut(U) + 'static,
{
    fn drop(&mut self) {
        self.state.borrow_mut().detach_value();
    }
}
