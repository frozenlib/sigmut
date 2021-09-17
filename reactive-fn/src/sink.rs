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

pub struct Sink2<S>(pub(crate) S);

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
