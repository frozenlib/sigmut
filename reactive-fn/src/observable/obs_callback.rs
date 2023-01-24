use super::*;
use crate::core::ObsContext;
use std::{marker::PhantomData, mem};

/// Output destination for observable value.
pub struct ObsSink<'cb, 'oc, 'a, T: ?Sized> {
    pub cb: ObsCallback<'cb, T>,
    pub oc: &'a mut ObsContext<'oc>,
}

impl<'cb, 'oc, 'a, T: ?Sized> ObsSink<'cb, 'oc, 'a, T> {
    pub fn ret(self, value: &T) -> Consumed<'cb> {
        self.cb.ret(value, self.oc)
    }
    pub fn ret_flat(self, o: &(impl Observable<Item = T> + ?Sized)) -> Consumed<'cb> {
        self.cb.ret_flat(o, self.oc)
    }
}

/// Something like trait object of `FnOnce(&T, &mut ObsContext)`.
///
/// Unlike `Box<dyn FnOnce(&T, &mut ObsContext)>`, no heap allocation.
pub struct ObsCallback<'cb, T: ?Sized>(&'cb mut dyn RawObsCallback<T>);

impl<'cb, T: ?Sized> ObsCallback<'cb, T> {
    pub fn ret(self, value: &T, oc: &mut ObsContext) -> Consumed<'cb> {
        self.0.ret(value, oc);
        Consumed::new()
    }
    pub fn ret_flat(
        self,
        o: &(impl Observable<Item = T> + ?Sized),
        oc: &mut ObsContext,
    ) -> Consumed<'cb> {
        o.with(|value, oc| self.ret(value, oc), oc)
    }
    pub fn context<'oc, 'a>(self, oc: &'a mut ObsContext<'oc>) -> ObsSink<'cb, 'oc, 'a, T> {
        ObsSink { cb: self, oc }
    }
}
impl<T: ?Sized> ObsCallback<'_, T> {
    pub fn with<R>(
        get: impl for<'a> FnOnce(ObsCallback<'a, T>) -> Consumed<'a>,
        f: impl FnOnce(&T, &mut ObsContext) -> R,
    ) -> R {
        let mut s = State::new(f);
        get(ObsCallback(&mut s));
        s.into_result()
    }
}

/// Something like trait object of `FnOnce(&T)`.
///
/// Unlike `Box<dyn FnOnce(&T)>`, no heap allocation.
pub struct Callback<'a, T: ?Sized>(&'a mut dyn RawCallback<T>);

impl<'a, T: ?Sized> Callback<'a, T> {
    pub fn ret(self, value: &T) -> Consumed<'a> {
        self.0.ret(value);
        Consumed::new()
    }
}
impl<T: ?Sized> Callback<'_, T> {
    pub fn with<R>(
        get: impl for<'a> FnOnce(Callback<'a, T>) -> Consumed<'a>,
        f: impl FnOnce(&T) -> R,
    ) -> R {
        let mut s = State::new(f);
        get(Callback(&mut s));
        s.into_result()
    }
}

/// Value that guarantees that the value identified by `'a` has been consumed.
pub struct Consumed<'a>(PhantomData<std::cell::Cell<&'a ()>>);

impl<'a> Consumed<'a> {
    fn new() -> Self {
        Consumed(PhantomData)
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
    fn ret(&mut self, value: &T, oc: &mut ObsContext);
}
impl<F, T, R> RawObsCallback<T> for State<F, R>
where
    T: ?Sized,
    F: FnOnce(&T, &mut ObsContext) -> R,
{
    fn ret(&mut self, value: &T, oc: &mut ObsContext) {
        self.apply(|f| f(value, oc))
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