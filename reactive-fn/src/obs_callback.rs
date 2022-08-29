use super::*;
use std::{marker::PhantomData, mem};

pub struct ObsContext<'a, 'b, 'bc, T: ?Sized> {
    pub cb: ObsCallback<'a, T>,
    pub bc: &'b mut BindContext<'bc>,
}

impl<'a, 'b, 'bc, T: ?Sized> ObsContext<'a, 'b, 'bc, T> {
    pub fn ret(self, value: &T) -> ObsRet<'a> {
        self.cb.ret(value, self.bc)
    }
    pub fn ret_flat(self, o: &impl Observable<Item = T>) -> ObsRet<'a> {
        self.cb.ret_flat(o, self.bc)
    }
}

pub struct ObsCallback<'a, T: ?Sized>(&'a mut dyn RawObsCallback<T>);

impl<'a, T: ?Sized> ObsCallback<'a, T> {
    pub fn ret(self, value: &T, bc: &mut BindContext) -> ObsRet<'a> {
        self.0.ret(value, bc);
        ObsRet::new()
    }
    pub fn ret_flat(self, o: &impl Observable<Item = T>, bc: &mut BindContext) -> ObsRet<'a> {
        o.with(|value, bc| self.ret(value, bc), bc)
    }
    pub fn context<'b, 'bc>(self, bc: &'b mut BindContext<'bc>) -> ObsContext<'a, 'b, 'bc, T> {
        ObsContext { cb: self, bc }
    }
}
impl<T: ?Sized> ObsCallback<'_, T> {
    pub fn with<R>(
        f0: impl for<'a> FnOnce(ObsCallback<'a, T>) -> ObsRet<'a>,
        f1: impl FnOnce(&T, &mut BindContext) -> R,
    ) -> R {
        let mut b = ObsCallbackBuilder::new(f1);
        f0(b.build());
        b.result()
    }
}

/// Type to ensure that [`ObsCallback`] is consumed.
pub struct ObsRet<'a>(PhantomData<std::cell::Cell<&'a ()>>);

impl<'a> ObsRet<'a> {
    fn new() -> Self {
        ObsRet(PhantomData)
    }
}

struct ObsCallbackBuilder<F, T: ?Sized, R> {
    state: State<F, R>,
    _phantom: PhantomData<fn(&T)>,
}

enum State<F, R> {
    FnOnce(F),
    Result(R),
    None,
}

impl<F, R> State<F, R> {
    fn take(&mut self) -> Self {
        mem::replace(self, State::None)
    }
}

trait RawObsCallback<T: ?Sized> {
    fn ret(&mut self, value: &T, bc: &mut BindContext);
}

impl<F, T, R> ObsCallbackBuilder<F, T, R>
where
    T: ?Sized,
    F: FnOnce(&T, &mut BindContext) -> R,
{
    pub fn new(f: F) -> Self {
        Self {
            state: State::FnOnce(f),
            _phantom: PhantomData,
        }
    }
    pub fn build(&mut self) -> ObsCallback<T> {
        ObsCallback(self)
    }
    pub fn result(mut self) -> R {
        if let State::Result(value) = self.state.take() {
            value
        } else {
            unreachable!()
        }
    }
}
impl<F, T, R> RawObsCallback<T> for ObsCallbackBuilder<F, T, R>
where
    T: ?Sized,
    F: FnOnce(&T, &mut BindContext) -> R,
{
    fn ret(&mut self, value: &T, bc: &mut BindContext) {
        if let State::FnOnce(f) = self.state.take() {
            self.state = State::Result(f(value, bc));
        } else {
            unreachable!()
        }
    }
}
