use crate::*;

pub trait Observer<T>: 'static {
    fn next(&mut self, value: T);
    fn into_dyn(self) -> DynObserver<T>
    where
        Self: Sized,
    {
        DynObserver(Some(Box::new(self)))
    }
}
impl<T, F: FnMut(T) -> () + 'static> Observer<T> for F {
    fn next(&mut self, value: T) {
        self(value)
    }
}

pub struct DynObserver<T>(Option<Box<dyn Observer<T>>>);

impl<T: 'static> Observer<T> for DynObserver<T> {
    fn next(&mut self, value: T) {
        if let Some(o) = &mut self.0 {
            o.next(value);
        }
    }
    fn into_dyn(self) -> DynObserver<T> {
        self
    }
}

pub trait Sink<T> {
    fn connect(self, value: T) -> DynObserver<T>;
}

impl<T, S> Sink<T> for Obs<S>
where
    T: Clone + 'static,
    S: Observable,
    S::Item: Sink<T>,
{
    fn connect(self, value: T) -> DynObserver<T> {
        let (head, tail) = self.head_tail();
        let o = head.connect(value.clone());
        if tail.is_empty() {
            o
        } else {
            OuterObserver(tail.subscribe_to(InnerObserver { o, value })).into_dyn()
        }
    }
}
struct InnerObserver<T> {
    value: T,
    o: DynObserver<T>,
}
impl<T: Clone + 'static, S: Sink<T>> Observer<S> for InnerObserver<T> {
    fn next(&mut self, value: S) {
        self.o = value.connect(self.value.clone());
    }
}
struct OuterObserver<S>(S);
impl<S: Subscriber<InnerObserver<T>>, T: Clone + 'static> Observer<T> for OuterObserver<S> {
    fn next(&mut self, value: T) {
        let mut b = self.0.borrow_mut();
        b.value = value.clone();
        b.o.next(value);
    }
}
