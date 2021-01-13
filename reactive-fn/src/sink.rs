use crate::*;
use std::rc::Rc;

pub struct DynObserver<T>(Option<Rc<dyn Observer<T>>>);

pub trait Sink<T> {
    fn connect(self, value: T) -> DynObserver<T>;
}

// impl<S: Observable<Item = T>, T: Sink<T>> Sink<T> for Obs<S> {
//     fn connect(self, value: T) -> DynObserver<T> {
//         let (head, tail) = self.head_tail();
//         let o = head.connect(value);

//     }
// }
