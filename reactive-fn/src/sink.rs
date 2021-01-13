use crate::*;
use std::rc::Rc;

pub struct Sink<T>(Option<Rc<dyn Observer<T>>>);

impl<T> Sink<T> {
    pub fn null() -> Self {
        Self(None)
    }
    pub fn new(o: impl Observer<T>) -> Self {
        todo!()
    }
}

pub trait IntoSink<T> {
    fn into_sink(self) -> Sink<T>;
}

impl<T, S: IntoSink<T>> IntoSink<T> for DynObs<S> {
    fn into_sink(self) -> Sink<T> {
        todo!()
    }
}
