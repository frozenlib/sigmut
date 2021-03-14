use super::*;
use std::borrow::Borrow;

#[derive(Clone)]
pub enum Source<T>
where
    T: Clone + 'static,
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
            Source::Constant(value) => {
                obs_constant_map_ref(value, |value| value.borrow()).into_dyn()
            }
            Source::Obs(o) => o.into_dyn(),
        }
    }
}

pub trait IntoSource<T: Clone> {
    fn into_source(self) -> Source<T>;
}

impl<S: Observable, U> IntoSource<U> for Obs<S>
where
    S::Item: Clone + Into<U>,
    U: Clone,
{
    fn into_source(self) -> Source<U> {
        self.map_into().into_dyn().into_source()
    }
}
impl<T, U> IntoSource<U> for DynObs<T>
where
    T: Clone + Into<U>,
    U: Clone,
{
    fn into_source(self) -> Source<U> {
        Source::Obs(self.map_into())
    }
}
impl<T, U> IntoSource<U> for &DynObs<T>
where
    T: Clone + Into<U>,
    U: Clone,
{
    fn into_source(self) -> Source<U> {
        self.clone().into_source()
    }
}

impl<T> IntoSource<T> for ObsCell<T>
where
    T: Clone,
{
    fn into_source(self) -> Source<T> {
        self.as_dyn().into_source()
    }
}
impl<T> IntoSource<T> for &ObsCell<T>
where
    T: Clone,
{
    fn into_source(self) -> Source<T> {
        self.clone().into_source()
    }
}

impl<T> IntoSource<ObsCell<T>> for ObsCell<T> {
    fn into_source(self) -> Source<ObsCell<T>> {
        Source::Constant(self)
    }
}
impl<T> IntoSource<ObsCell<T>> for &ObsCell<T> {
    fn into_source(self) -> Source<ObsCell<T>> {
        self.clone().into_source()
    }
}
impl<T, U> IntoSource<Option<U>> for Option<T>
where
    T: IntoSource<U>,
    U: Clone + 'static,
{
    fn into_source(self) -> Source<Option<U>> {
        if let Some(s) = self {
            s.into_source().map(Some)
        } else {
            Source::Constant(None)
        }
    }
}
impl<T0, T1, E0, E1> IntoSource<Result<T1, E1>> for Result<T0, E0>
where
    T0: IntoSource<T1>,
    T1: Clone + 'static,
    E0: IntoSource<E1>,
    E1: Clone + 'static,
{
    fn into_source(self) -> Source<Result<T1, E1>> {
        match self {
            Ok(s) => s.into_source().map(Ok),
            Err(s) => s.into_source().map(Err),
        }
    }
}

macro_rules! impl_into_source {
    ($($t:ty),*) => { $(
        impl IntoSource<$t> for $t {
            fn into_source(self) -> Source<$t> {
                Source::Constant(self)
            }
        }
    )*
    };
}
impl_into_source!(u8, u16, u32, u64, u128, usize);
impl_into_source!(i8, i16, i32, i64, i128, isize);
impl_into_source!(f32, f64);
impl_into_source!(bool, char);
