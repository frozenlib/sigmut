use super::*;
use std::{marker::PhantomData, mem};

pub struct ObsContext<'a, 'b, 'bc, T: ?Sized> {
    pub cb: ObsCallback<'a, T>,
    pub bc: &'b mut BindContext<'bc>,
}

impl<'a, 'b, 'bc, T: ?Sized> ObsContext<'a, 'b, 'bc, T> {
    pub fn ret(self, value: &T) -> Ret<'a> {
        self.cb.ret(value, self.bc)
    }
    pub fn ret_flat(self, o: &impl Observable<Item = T>) -> Ret<'a> {
        self.cb.ret_flat(o, self.bc)
    }
}

pub struct ObsCallback<'a, T: ?Sized>(&'a mut dyn RawObsCallback<T>);

impl<'a, T: ?Sized> ObsCallback<'a, T> {
    pub fn ret(self, value: &T, bc: &mut BindContext) -> Ret<'a> {
        self.0.ret(value, bc);
        Ret::new()
    }
    pub fn ret_flat(self, o: &impl Observable<Item = T>, bc: &mut BindContext) -> Ret<'a> {
        o.with(|value, bc| self.ret(value, bc), bc)
    }
    pub fn context<'b, 'bc>(self, bc: &'b mut BindContext<'bc>) -> ObsContext<'a, 'b, 'bc, T> {
        ObsContext { cb: self, bc }
    }
}
impl<T: ?Sized> ObsCallback<'_, T> {
    pub fn with<R>(
        f0: impl for<'a> FnOnce(ObsCallback<'a, T>) -> Ret<'a>,
        f1: impl FnOnce(&T, &mut BindContext) -> R,
    ) -> R {
        let mut s = State::new(f1);
        f0(ObsCallback(&mut s));
        s.into_result()
    }
}

pub struct Callback<'a, T: ?Sized>(&'a mut dyn RawCallback<T>);

impl<'a, T: ?Sized> Callback<'a, T> {
    pub fn ret(self, value: &T) -> Ret<'a> {
        self.0.ret(value);
        Ret::new()
    }
}
impl<T: ?Sized> Callback<'_, T> {
    pub fn with<R>(
        f0: impl for<'a> FnOnce(Callback<'a, T>) -> Ret<'a>,
        f1: impl FnOnce(&T) -> R,
    ) -> R {
        let mut s = State::new(f1);
        f0(Callback(&mut s));
        s.into_result()
    }
}

/// Type to ensure that [`Callback`] or [`ObsCallback`] was consumed.
pub struct Ret<'a>(PhantomData<std::cell::Cell<&'a ()>>);

impl<'a> Ret<'a> {
    fn new() -> Self {
        Ret(PhantomData)
    }
}

enum State<F, R> {
    FnOnce(F),
    Result(R),
    None,
}
impl<F, R> State<F, R> {
    fn new(f: F) -> Self {
        Self::FnOnce(f)
    }
    fn apply(&mut self, f: impl FnOnce(F) -> R) {
        if let Self::FnOnce(f0) = mem::replace(self, State::None) {
            *self = Self::Result(f(f0));
        } else {
            unreachable!()
        }
    }
    fn into_result(self) -> R {
        if let Self::Result(r) = self {
            r
        } else {
            unreachable!()
        }
    }
}

trait RawObsCallback<T: ?Sized> {
    fn ret(&mut self, value: &T, bc: &mut BindContext);
}
impl<F, T, R> RawObsCallback<T> for State<F, R>
where
    T: ?Sized,
    F: FnOnce(&T, &mut BindContext) -> R,
{
    fn ret(&mut self, value: &T, bc: &mut BindContext) {
        self.apply(|f| f(value, bc))
    }
}

trait RawCallback<T: ?Sized> {
    fn ret(&mut self, value: &T);
}
impl<F, T, R> RawCallback<T> for State<F, R>
where
    T: ?Sized,
    F: FnOnce(&T) -> R,
{
    fn ret(&mut self, value: &T) {
        self.apply(|f| f(value))
    }
}
