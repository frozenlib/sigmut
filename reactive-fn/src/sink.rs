use super::*;

pub trait IntoSink<T> {
    type RawSink: RawSink<Item = T>;
    fn into_sink(self) -> Sink<Self::RawSink>;
}

pub trait RawSink: 'static {
    type Item;
    type Observer: Observer<Self::Item>;
    fn connect(&self, value: Self::Item) -> Self::Observer;
}

#[derive(Clone)]
pub struct Sink<S>(pub(crate) S);

impl<S> Sink<S> {
    pub fn into_raw(self) -> S {
        self.0
    }
}
impl<S: RawSink> RawSink for Sink<S> {
    type Item = S::Item;
    type Observer = S::Observer;

    fn connect(&self, value: Self::Item) -> Self::Observer {
        self.0.connect(value)
    }
}

impl<S: RawSink> IntoSink<S::Item> for Sink<S> {
    type RawSink = S;
    fn into_sink(self) -> Sink<Self::RawSink> {
        self
    }
}
impl<S: RawSink + Clone> IntoSink<S::Item> for &Sink<S> {
    type RawSink = S;
    fn into_sink(self) -> Sink<Self::RawSink> {
        self.clone().into_sink()
    }
}
