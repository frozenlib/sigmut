use super::*;

pub trait IntoSink2<T> {
    type RawSink: RawSink2<Item = T>;
    fn into_sink(self) -> Sink2<Self::RawSink>;
}

pub trait RawSink2 {
    type Item;
    type Observer: Observer<Self::Item>;
    fn next(&self, value: Self::Item, bc: &mut BindContext);
    fn connect(self, value: Self::Item) -> Self::Observer;
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
    S: Observable,
    S::Item: RawSink2,
    <S::Item as RawSink2>::Item: Clone,
{
    type Item = <S::Item as RawSink2>::Item;
    type Observer = DynSubscriber<ObsCell<Self::Item>>;

    fn next(&self, value: Self::Item, bc: &mut BindContext) {
        self.with(|item, bc| item.next(value, bc), bc)
    }
    fn connect(self, value: Self::Item) -> Self::Observer {
        subscribe_to(ObsCell::new(value), move |st, bc| {
            self.with(|sink, bc| sink.next(st.get(bc), bc), bc)
        })
        .into_dyn()
    }
}
impl<S> IntoSink2<<S::Item as RawSink2>::Item> for Obs<S>
where
    S: Observable,
    S::Item: RawSink2,
    <S::Item as RawSink2>::Item: Clone,
{
    type RawSink = Self;

    fn into_sink(self) -> Sink2<Self::RawSink> {
        Sink2(self)
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
