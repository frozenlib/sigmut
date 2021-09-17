use super::*;

pub trait IntoSink2<T> {
    type RawSink: RawSink2<Item = T>;
    fn into_sink(self) -> Sink2<Self::RawSink>;
}

pub trait RawSink2: 'static {
    type Item;
    type Observer: Observer<Self::Item>;
    fn connect(&self, value: Self::Item) -> Self::Observer;
}

pub struct Sink2<S>(S);

impl<S> Sink2<S> {
    pub fn into_raw(self) -> S {
        self.0
    }
}
impl<S: RawSink2> IntoSink2<S::Item> for Sink2<S> {
    type RawSink = S;
    fn into_sink(self) -> Sink2<Self::RawSink> {
        self
    }
}

impl<S> RawSink2 for Obs<S>
where
    S: Observable + Clone,
    S::Item: RawSink2,
    <S::Item as RawSink2>::Item: Clone,
{
    type Item = <S::Item as RawSink2>::Item;
    type Observer = ObsSinkObserver<Self::Item, <S::Item as RawSink2>::Observer>;

    fn connect(&self, value: Self::Item) -> Self::Observer {
        ObsSinkObserver(
            self.clone()
                .subscribe_to(ObsSinkState { value, o: None })
                .into_dyn(),
        )
    }
}

pub struct ObsSinkObserver<T, O>(DynSubscriber<ObsSinkState<T, O>>);
impl<T, O> Observer<T> for ObsSinkObserver<T, O>
where
    T: Clone + 'static,
    O: Observer<T>,
{
    fn next(&mut self, value: T) {
        let mut b = self.0.borrow_mut();
        b.value = value.clone();
        if let Some(o) = &mut b.o {
            o.next(value);
        }
    }
}

struct ObsSinkState<T, O> {
    value: T,
    o: Option<O>,
}
impl<'a, S> Observer<&'a S> for ObsSinkState<S::Item, S::Observer>
where
    S: ?Sized + RawSink2,
    S::Item: Clone,
{
    fn next(&mut self, value: &'a S) {
        self.o = Some(value.connect(self.value.clone()));
    }
}

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
