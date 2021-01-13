use crate::*;
use std::rc::Rc;

pub struct DynObserver<T>(Option<Rc<dyn Observer<T>>>);

pub trait Sink<T> {
    fn connect(self, value: T) -> DynObserver<T>;
}
