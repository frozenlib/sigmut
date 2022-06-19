use std::rc::Rc;

use either::Either;
pub trait Observer<T>: 'static {
    fn next(&mut self, value: T);
    fn into_dyn(self) -> DynObserver<T>
    where
        Self: Sized,
    {
        DynObserver::from_box(Box::new(self))
    }
}
impl<T, F: FnMut(T) + 'static> Observer<T> for F {
    fn next(&mut self, value: T) {
        self(value)
    }
}
impl<T, L, R> Observer<T> for Either<L, R>
where
    L: Observer<T>,
    R: Observer<T>,
{
    fn next(&mut self, value: T) {
        match self {
            Either::Left(l) => l.next(value),
            Either::Right(r) => r.next(value),
        }
    }
}

pub struct DynObserver<T>(RawDynObserver<T>);

enum RawDynObserver<T> {
    Null,
    Box(Box<dyn Observer<T>>),
    Rc(Rc<dyn RcObserver<T>>),
}

impl<T> DynObserver<T> {
    pub fn from_box(o: Box<dyn Observer<T>>) -> Self {
        Self(RawDynObserver::Box(o))
    }
    pub(crate) fn from_rc(o: Rc<dyn RcObserver<T>>) -> Self {
        Self(RawDynObserver::Rc(o))
    }
    pub fn null() -> Self {
        Self(RawDynObserver::Null)
    }
    pub fn is_null(&self) -> bool {
        matches!(self.0, RawDynObserver::Null)
    }
}

impl<T: 'static> Observer<T> for DynObserver<T> {
    fn next(&mut self, value: T) {
        match &mut self.0 {
            RawDynObserver::Null => {}
            RawDynObserver::Box(o) => o.next(value),
            RawDynObserver::Rc(o) => o.clone().next(value),
        }
    }
    fn into_dyn(self) -> DynObserver<T> {
        self
    }
}
pub(crate) trait RcObserver<T> {
    fn next(self: Rc<Self>, value: T);
}
