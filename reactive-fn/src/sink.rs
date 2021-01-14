use crate::*;

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
impl<T, S> Sink<T> for ObsBorrow<S>
where
    T: Clone + 'static,
    S: ObservableBorrow,
    for<'a> &'a S::Item: Sink<T>,
{
    fn connect(self, value: T) -> DynObserver<T> {
        self.as_ref().connect(value)
    }
}

impl<T, S> Sink<T> for ObsRef<S>
where
    T: Clone + 'static,
    S: ObservableRef,
    for<'a> &'a S::Item: Sink<T>,
{
    fn connect(self, value: T) -> DynObserver<T> {
        let (o, tail) = self.head_tail(|head| head.connect(value.clone()));
        if tail.is_empty() {
            o
        } else {
            OuterObserver(tail.subscribe_to(InnerObserver { o, value })).into_dyn()
        }
    }
}

impl<T, S> Sink<T> for DynObs<S>
where
    T: Clone + 'static,
    S: Sink<T>,
{
    fn connect(self, value: T) -> DynObserver<T> {
        (&self).connect(value)
    }
}
impl<T, S> Sink<T> for &DynObs<S>
where
    T: Clone + 'static,
    S: Sink<T>,
{
    fn connect(self, value: T) -> DynObserver<T> {
        self.obs().connect(value)
    }
}

impl<T, S> Sink<T> for DynObsBorrow<S>
where
    T: Clone + 'static,
    for<'a> &'a S: Sink<T>,
{
    fn connect(self, value: T) -> DynObserver<T> {
        (&self).connect(value)
    }
}
impl<T, S> Sink<T> for &DynObsBorrow<S>
where
    T: Clone + 'static,
    for<'a> &'a S: Sink<T>,
{
    fn connect(self, value: T) -> DynObserver<T> {
        self.as_ref().connect(value)
    }
}

impl<T, S> Sink<T> for DynObsRef<S>
where
    T: Clone + 'static,
    for<'a> &'a S: Sink<T>,
{
    fn connect(self, value: T) -> DynObserver<T> {
        (&self).connect(value)
    }
}
impl<T, S> Sink<T> for &DynObsRef<S>
where
    T: Clone + 'static,
    for<'a> &'a S: Sink<T>,
{
    fn connect(self, value: T) -> DynObserver<T> {
        self.obs().connect(value)
    }
}

// impl<T,S> Sink<T> for Source<S>
// where
//     T: Clone + 'static,
//     for<'a> &'a T: Sink<T>,
// {
//     fn connect(self, value: T) -> DynObserver<T> {
//         match self {
//             Source::Constant(c) => c.connect(value),
//             Source::Obs(o) => o.connect(value),
//         }
//     }
// }
// impl<T> Sink<T> for &Source<T>
// where
//     T: Clone + 'static,
//     for<'a> &'a T: Sink<T>,
// {
//     fn connect(self, value: T) -> DynObserver<T> {
//         match self {
//             Source::Constant(c) => c.connect(value),
//             Source::Obs(o) => o.as_ref().connect(value),
//         }
//     }
// }

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
