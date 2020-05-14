use crate::bind::*;
use crate::reactive::*;
use std::{cell::RefCell, rc::Rc};

pub struct ForEach<T: 'static, F> {
    source: Re<T>,
    state: RefCell<ForEachState<F>>,
}
struct ForEachState<F> {
    f: F,
    bindings: Vec<Binding>,
}

impl<T: 'static, F: FnMut(T) + 'static> ForEach<T, F> {
    pub fn new(source: Re<T>, f: F) -> Rc<Self> {
        let s = Rc::new(ForEach {
            source,
            state: RefCell::new(ForEachState {
                f,
                bindings: Vec::new(),
            }),
        });
        s.next();
        s
    }

    fn next(self: &Rc<Self>) {
        let b = &mut *self.state.borrow_mut();
        let mut ctx = ReactiveContext::new(&self, &mut b.bindings);
        (b.f)(self.source.get(&mut ctx));
    }
}
impl<T: 'static, F: FnMut(T) + 'static> BindSink for ForEach<T, F> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        self.state.borrow_mut().bindings.clear();
        ctx.spawn(Rc::downgrade(&self))
    }
}
impl<T: 'static, F: FnMut(T) + 'static> Task for ForEach<T, F> {
    fn run(self: Rc<Self>) {
        self.next();
    }
}

pub struct ForEachBy<T, U, A, D>
where
    T: 'static,
    U: 'static,
    A: Fn(T) -> U + 'static,
    D: Fn(U) + 'static,
{
    source: Re<T>,
    attach: A,
    detach: D,
    value: RefCell<Option<U>>,
    bindings: RefCell<Vec<Binding>>,
}

impl<T, U, A, D> ForEachBy<T, U, A, D>
where
    T: 'static,
    U: 'static,
    A: Fn(T) -> U + 'static,
    D: Fn(U) + 'static,
{
    pub fn new(source: Re<T>, attach: A, detach: D) -> Rc<Self> {
        let s = Rc::new(ForEachBy {
            source,
            attach,
            detach,
            value: RefCell::new(None),
            bindings: RefCell::new(Vec::new()),
        });
        s.next();
        s
    }

    fn next(self: &Rc<Self>) {
        let mut b = self.bindings.borrow_mut();
        let mut ctx = ReactiveContext::new(&self, &mut b);
        *self.value.borrow_mut() = Some((self.attach)(self.source.get(&mut ctx)));
    }
    fn detach_value(&self) {
        if let Some(value) = self.value.borrow_mut().take() {
            (self.detach)(value);
        }
    }
}
impl<T, U, A, D> BindSink for ForEachBy<T, U, A, D>
where
    T: 'static,
    U: 'static,
    A: Fn(T) -> U + 'static,
    D: Fn(U) + 'static,
{
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        self.bindings.borrow_mut().clear();
        self.detach_value();
        ctx.spawn(Rc::downgrade(&self))
    }
}
impl<T, U, A, D> Task for ForEachBy<T, U, A, D>
where
    T: 'static,
    U: 'static,
    A: Fn(T) -> U + 'static,
    D: Fn(U) + 'static,
{
    fn run(self: Rc<Self>) {
        self.next();
    }
}
impl<T, U, A, D> Drop for ForEachBy<T, U, A, D>
where
    T: 'static,
    U: 'static,
    A: Fn(T) -> U + 'static,
    D: Fn(U) + 'static,
{
    fn drop(&mut self) {
        self.detach_value();
    }
}
