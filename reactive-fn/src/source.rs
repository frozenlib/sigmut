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
    pub fn get(&self, cx: &mut BindContext) -> T {
        self.with(|value, _| value.to_owned(), cx)
    }
    pub fn get_head(&self) -> T {
        BindContext::nul(|cx| self.get(cx))
    }

    pub fn with<U>(&self, f: impl FnOnce(&T, &mut BindContext) -> U, cx: &mut BindContext) -> U {
        match self {
            Self::Constant(value) => f(value.borrow(), cx),
            Self::Obs(obs) => obs.with(|value, cx| f(value, cx), cx),
        }
    }
    pub fn with_head<U>(&self, f: impl FnOnce(&T) -> U) -> U {
        BindContext::nul(|cx| self.with(|value, _| f(value), cx))
    }

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
        Source::with(self, f, cx)
    }
    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        match self {
            Source::Constant(value) => DynObs::new_constant_map_ref(value, |value| value.borrow()),
            Source::Obs(o) => o.into_dyn(),
        }
    }
}

impl<S, U> From<Obs<S>> for Source<U>
where
    S: Observable,
    S::Item: Clone + Into<U>,
    U: Clone,
{
    fn from(value: Obs<S>) -> Self {
        value.map_into().into_dyn().into()
    }
}

impl<T, U> From<DynObs<T>> for Source<U>
where
    T: Clone + Into<U>,
    U: Clone,
{
    fn from(value: DynObs<T>) -> Self {
        Source::Obs(value.map_into())
    }
}
impl<T, U> From<&DynObs<T>> for Source<U>
where
    T: Clone + Into<U>,
    U: Clone,
{
    fn from(value: &DynObs<T>) -> Self {
        value.clone().into()
    }
}

impl<T> From<ObsCell<T>> for Source<ObsCell<T>> {
    fn from(value: ObsCell<T>) -> Source<ObsCell<T>> {
        Source::Constant(value)
    }
}
impl<T> From<&ObsCell<T>> for Source<ObsCell<T>> {
    fn from(value: &ObsCell<T>) -> Source<ObsCell<T>> {
        value.clone().into()
    }
}

impl<T: Copy> From<ObsCell<T>> for Source<T> {
    fn from(value: ObsCell<T>) -> Self {
        value.obs().into()
    }
}
impl<T: Copy> From<&ObsCell<T>> for Source<T> {
    fn from(value: &ObsCell<T>) -> Self {
        value.clone().into()
    }
}

impl<T> From<Option<T>> for Source<Option<T>>
where
    T: Clone + Into<Source<T>> + 'static,
{
    fn from(value: Option<T>) -> Self {
        Source::Constant(value)
    }
}
impl<T, E> From<Result<T, E>> for Source<Result<T, E>>
where
    T: Clone + Into<Source<T>> + 'static,
    E: Clone + Into<Source<E>> + 'static,
{
    fn from(value: Result<T, E>) -> Source<Result<T, E>> {
        Source::Constant(value)
    }
}

macro_rules! impl_from_for_source {
    ($($t:ty),*) => { $(
        impl<T: Into<$t>> From<T> for Source<$t> {
            fn from(value: T) -> Self {
                Self::Constant(value.into())
            }
        }
    )*
    };
}
impl_from_for_source!(u8, u16, u32, u64, u128, usize);
impl_from_for_source!(i8, i16, i32, i64, i128, isize);
impl_from_for_source!(f32, f64);
impl_from_for_source!(bool, char);
