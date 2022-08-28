use super::*;
use std::{marker::PhantomData, mem};

pub struct ObserverContext<'a, 'b, 'bc, T: ?Sized> {
    pub f: DynOnceObserver<'a, T>,
    pub bc: &'b mut BindContext<'bc>,
}

impl<'a, 'b, 'bc, T: ?Sized> ObserverContext<'a, 'b, 'bc, T> {
    pub fn ret(self, value: &T) -> ObserverResult<'a> {
        self.f.ret(value, self.bc)
    }
    pub fn ret_flat(self, o: &impl Observable<Item = T>) -> ObserverResult<'a> {
        self.f.ret_flat(o, self.bc)
    }
}

pub struct DynOnceObserver<'a, T: ?Sized>(&'a mut dyn RawDynOnceObserver<T>);

impl<'a, T: ?Sized> DynOnceObserver<'a, T> {
    pub fn ret(self, value: &T, bc: &mut BindContext) -> ObserverResult<'a> {
        self.0.ret(value, bc);
        ObserverResult::new()
    }
    pub fn ret_flat(
        self,
        o: &impl Observable<Item = T>,
        bc: &mut BindContext,
    ) -> ObserverResult<'a> {
        o.with(|value, bc| self.ret(value, bc), bc)
    }
}
pub struct ObserverResult<'a> {
    _phantom: PhantomData<&'a mut ()>,
}

impl<'a> ObserverResult<'a> {
    fn new() -> Self {
        ObserverResult {
            _phantom: PhantomData,
        }
    }
}

pub struct DynOnceObserverBuilder<F, T: ?Sized, R> {
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

trait RawDynOnceObserver<T: ?Sized> {
    fn ret(&mut self, value: &T, bc: &mut BindContext);
}

impl<F, T, R> DynOnceObserverBuilder<F, T, R>
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
    pub fn build(&mut self) -> DynOnceObserver<T> {
        DynOnceObserver(self)
    }
    pub fn result(mut self) -> R {
        if let State::Result(value) = self.state.take() {
            value
        } else {
            panic!("`OnceObserver::ret` was not called.");
        }
    }
}
impl<F, T, R> RawDynOnceObserver<T> for DynOnceObserverBuilder<F, T, R>
where
    T: ?Sized,
    F: FnOnce(&T, &mut BindContext) -> R,
{
    fn ret(&mut self, value: &T, bc: &mut BindContext) {
        if let State::FnOnce(f) = self.state.take() {
            self.state = State::Result(f(value, bc));
        } else {
            panic!("`OnceObserver::ret` called twice.");
        }
    }
}
