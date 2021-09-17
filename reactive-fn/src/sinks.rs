use super::*;
use either::Either;

impl<S> RawSink2 for Obs<S>
where
    S: Observable + Clone,
    S::Item: RawSink2,
    <S::Item as RawSink2>::Item: Clone,
{
    type Item = <S::Item as RawSink2>::Item;
    type Observer = ObsSinkObserver<S::Item>;

    fn connect(&self, value: Self::Item) -> Self::Observer {
        ObsSinkObserver(
            self.clone()
                .subscribe_to(ObsSinkState { value, o: None })
                .into_dyn(),
        )
    }
}

impl<T> RawSink2 for DynObs<T>
where
    T: ?Sized + RawSink2,
    T::Item: Clone,
{
    type Item = T::Item;
    type Observer = ObsSinkObserver<T>;

    fn connect(&self, value: Self::Item) -> Self::Observer {
        ObsSinkObserver(self.subscribe_to(ObsSinkState { value, o: None }))
    }
}
impl<T> RawSink2 for MayObs<T>
where
    T: RawSink2,
    T::Item: Clone,
{
    type Item = T::Item;
    type Observer = Either<T::Observer, <DynObs<T> as RawSink2>::Observer>;

    fn connect(&self, value: Self::Item) -> Self::Observer {
        match self {
            MayObs::Constant(s) => Either::Left(s.connect(value)),
            MayObs::Obs(s) => Either::Right(s.connect(value)),
        }
    }
}

pub struct ObsSinkObserver<S: ?Sized + RawSink2>(DynSubscriber<ObsSinkState<S::Item, S::Observer>>);
impl<S> Observer<S::Item> for ObsSinkObserver<S>
where
    S: ?Sized + RawSink2,
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
    S: ?Sized + RawSink2,
    S::Item: Clone,
{
    fn next(&mut self, value: &'a S) {
        self.o = Some(value.connect(self.value.clone()));
    }
}

impl<S> IntoSink2<<S::Item as RawSink2>::Item> for Obs<S>
where
    S: Observable,
    S::Item: RawSink2,
    <S::Item as RawSink2>::Item: Clone,
{
    type RawSink = DynObs<S::Item>;

    fn into_sink(self) -> Sink2<Self::RawSink> {
        self.into_dyn().into_sink()
    }
}
impl<S> IntoSink2<<S::Item as RawSink2>::Item> for &Obs<S>
where
    S: Observable + Clone,
    S::Item: RawSink2,
    <S::Item as RawSink2>::Item: Clone,
{
    type RawSink = DynObs<S::Item>;

    fn into_sink(self) -> Sink2<Self::RawSink> {
        self.clone().into_sink()
    }
}

impl<T> IntoSink2<T::Item> for DynObs<T>
where
    T: ?Sized + RawSink2,
    T::Item: Clone,
{
    type RawSink = Self;

    fn into_sink(self) -> Sink2<Self::RawSink> {
        Sink2(self)
    }
}
impl<T> IntoSink2<T::Item> for &DynObs<T>
where
    T: ?Sized + RawSink2,
    T::Item: Clone,
{
    type RawSink = DynObs<T>;

    fn into_sink(self) -> Sink2<Self::RawSink> {
        self.clone().into_sink()
    }
}
impl<T> IntoSink2<T::Item> for MayObs<T>
where
    T: RawSink2,
    T::Item: Clone,
{
    type RawSink = Self;

    fn into_sink(self) -> Sink2<Self::RawSink> {
        Sink2(self)
    }
}
impl<T> IntoSink2<T::Item> for &MayObs<T>
where
    T: RawSink2 + Clone,
    T::Item: Clone,
{
    type RawSink = MayObs<T>;

    fn into_sink(self) -> Sink2<Self::RawSink> {
        self.clone().into_sink()
    }
}
