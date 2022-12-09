use super::*;
use either::Either;

impl<S> IntoSink<<S::Item as RawSink>::Item> for ImplObs<S>
where
    S: Observable + 'static,
    S::Item: RawSink,
    <S::Item as RawSink>::Item: Clone,
{
    type RawSink = DynObs<S::Item>;

    fn into_sink(self) -> Sink<Self::RawSink> {
        self.into_dyn().into_sink()
    }
}
impl<S> IntoSink<<S::Item as RawSink>::Item> for &ImplObs<S>
where
    S: Observable + Clone + 'static,
    S::Item: RawSink,
    <S::Item as RawSink>::Item: Clone,
{
    type RawSink = DynObs<S::Item>;

    fn into_sink(self) -> Sink<Self::RawSink> {
        self.clone().into_sink()
    }
}

impl<T> RawSink for DynObs<T>
where
    T: ?Sized + RawSink,
    T::Item: Clone,
{
    type Item = T::Item;
    type Observer = ObsSinkObserver<T>;

    fn connect(&self, value: Self::Item) -> Self::Observer {
        ObsSinkObserver(self.subscribe_to(ObsSinkState { value, o: None }))
    }
}
impl<T> RawSink for MayObs<T>
where
    T: RawSink,
    T::Item: Clone,
{
    type Item = T::Item;
    type Observer = Either<T::Observer, <DynObs<T> as RawSink>::Observer>;

    fn connect(&self, value: Self::Item) -> Self::Observer {
        match self {
            MayObs::Constant(s) => Either::Left(s.connect(value)),
            MayObs::Obs(s) => Either::Right(s.connect(value)),
        }
    }
}

pub struct ObsSinkObserver<S: ?Sized + RawSink>(DynSubscriber<ObsSinkState<S::Item, S::Observer>>);
impl<S> Observer<S::Item> for ObsSinkObserver<S>
where
    S: ?Sized + RawSink,
    S::Item: Clone,
{
    fn next(&mut self, value: S::Item) {
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
    S: ?Sized + RawSink,
    S::Item: Clone,
{
    fn next(&mut self, value: &'a S) {
        self.o = Some(value.connect(self.value.clone()));
    }
}

impl<T> IntoSink<T::Item> for DynObs<T>
where
    T: ?Sized + RawSink,
    T::Item: Clone,
{
    type RawSink = Self;

    fn into_sink(self) -> Sink<Self::RawSink> {
        Sink(self)
    }
}
impl<T> IntoSink<T::Item> for &DynObs<T>
where
    T: ?Sized + RawSink,
    T::Item: Clone,
{
    type RawSink = DynObs<T>;

    fn into_sink(self) -> Sink<Self::RawSink> {
        self.clone().into_sink()
    }
}
impl<T> IntoSink<T::Item> for MayObs<T>
where
    T: RawSink,
    T::Item: Clone,
{
    type RawSink = Self;

    fn into_sink(self) -> Sink<Self::RawSink> {
        Sink(self)
    }
}
impl<T> IntoSink<T::Item> for &MayObs<T>
where
    T: RawSink + Clone,
    T::Item: Clone,
{
    type RawSink = MayObs<T>;

    fn into_sink(self) -> Sink<Self::RawSink> {
        self.clone().into_sink()
    }
}
