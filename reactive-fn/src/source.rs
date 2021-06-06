use super::*;
use std::borrow::Borrow;

#[derive(Clone)]
pub enum Source<T>
where
    T: 'static,
{
    Constant(T),
    Obs(DynObs<T>),
}

impl<T> Source<T>
where
    T: Clone + 'static,
{
    pub fn obs(&self) -> Obs<impl Observable<Item = T>> {
        Obs(self.clone())
    }

    pub fn map<U>(self, f: impl Fn(T) -> U + 'static) -> Source<U>
    where
        U: Clone,
    {
        match self {
            Source::Constant(value) => Source::Constant(f(value)),
            Source::Obs(o) => Source::Obs(o.map(move |value| f(value.clone()))),
        }
    }
}
impl<T> Observable for Source<T>
where
    T: Clone + 'static,
{
    type Item = T;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        match self {
            Self::Constant(value) => f(value.borrow(), cx),
            Self::Obs(obs) => obs.with(|value, cx| f(value, cx), cx),
        }
    }
    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        match self {
            Source::Constant(value) => obs_constant(value).into_dyn(),
            Source::Obs(o) => o.into_dyn(),
        }
    }
    fn into_source(self) -> Source<Self::Item> {
        self
    }
}
