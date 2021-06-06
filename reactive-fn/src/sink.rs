use super::*;

pub trait Sink<T> {
    fn connect(self, value: T) -> DynObserver<T>;
}
impl<T, S> Sink<T> for Obs<S>
where
    T: Clone + 'static,
    S: Observable,
    for<'a> &'a S::Item: Sink<T>,
{
    fn connect(self, value: T) -> DynObserver<T> {
        let (o, tail) = self.with_head_tail(|head| head.connect(value.clone()));
        if tail.is_empty() {
            o
        } else {
            OuterObserver(tail.subscribe_to(InnerObserver { o, value })).into_dyn()
        }
    }
}
impl<T, S> Sink<T> for &Obs<S>
where
    T: Clone + 'static,
    S: Observable + Clone,
    for<'a> &'a S::Item: Sink<T>,
{
    fn connect(self, value: T) -> DynObserver<T> {
        self.clone().connect(value)
    }
}

impl<T, S> Sink<T> for DynObs<S>
where
    T: Clone + 'static,
    for<'a> &'a S: Sink<T>,
{
    fn connect(self, value: T) -> DynObserver<T> {
        (&self).connect(value)
    }
}
impl<T, S> Sink<T> for &DynObs<S>
where
    T: Clone + 'static,
    for<'a> &'a S: Sink<T>,
{
    fn connect(self, value: T) -> DynObserver<T> {
        self.obs().connect(value)
    }
}

impl<T, S> Sink<T> for MayObs<S>
where
    T: Clone + 'static,
    S: Clone,
    for<'a> &'a S: Sink<T>,
{
    fn connect(self, value: T) -> DynObserver<T> {
        match self {
            MayObs::Constant(c) => c.connect(value),
            MayObs::Obs(o) => o.connect(value),
        }
    }
}
impl<T, S> Sink<T> for &MayObs<S>
where
    T: Clone + 'static,
    S: Clone,
    for<'a> &'a S: Sink<T>,
{
    fn connect(self, value: T) -> DynObserver<T> {
        match self {
            MayObs::Constant(c) => c.connect(value),
            MayObs::Obs(o) => o.connect(value),
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
impl<S: Subscriber<St = InnerObserver<T>>, T: Clone + 'static> Observer<T> for OuterObserver<S> {
    fn next(&mut self, value: T) {
        let mut b = self.0.borrow_mut();
        b.value = value.clone();
        b.o.next(value);
    }
}
