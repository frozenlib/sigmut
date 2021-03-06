use super::*;
use std::{borrow::Borrow, rc::Rc, sync::Arc};

#[derive(Clone)]
pub enum Source<T>
where
    T: ?Sized + ToOwned + 'static,
{
    Constant(T::Owned),
    Obs(DynObs<T>),
}

impl<T> Source<T>
where
    T: ?Sized + ToOwned + 'static,
{
    pub fn obs(self) -> Obs<impl Observable<Item = T>> {
        Obs(self)
    }
    pub fn map<U>(self, f: impl Fn(&T) -> U + 'static) -> Source<U>
    where
        T: Sized + ToOwned<Owned = T>,
        U: Clone,
    {
        match self {
            Source::Constant(value) => Source::Constant(f(value.borrow())),
            Source::Obs(o) => Source::Obs(o.map(f)),
        }
    }
}
impl<T> Observable for Source<T>
where
    T: ?Sized + ToOwned + 'static,
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
            Source::Constant(value) => DynObs::new_constant_map_ref(value, |value| value.borrow()),
            Source::Obs(o) => o.into_dyn(),
        }
    }
}

impl<S> From<Obs<S>> for Source<S::Item>
where
    S: Observable,
    S::Item: ToOwned,
{
    fn from(value: Obs<S>) -> Self {
        value.into_dyn().into()
    }
}

impl<T: ?Sized + ToOwned> From<DynObs<T>> for Source<T> {
    fn from(value: DynObs<T>) -> Self {
        Source::Obs(value)
    }
}
impl<T: ?Sized + ToOwned> From<&DynObs<T>> for Source<T> {
    fn from(value: &DynObs<T>) -> Self {
        value.clone().into()
    }
}

impl<T: ?Sized> From<Rc<T>> for Source<Rc<T>> {
    fn from(value: Rc<T>) -> Self {
        Source::Constant(value)
    }
}
impl<T: ?Sized> From<&Rc<T>> for Source<Rc<T>> {
    fn from(value: &Rc<T>) -> Self {
        value.clone().into()
    }
}
impl<T> From<Arc<T>> for Source<Arc<T>> {
    fn from(value: Arc<T>) -> Self {
        Source::Constant(value)
    }
}
impl<T> From<&Arc<T>> for Source<Arc<T>> {
    fn from(value: &Arc<T>) -> Self {
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

#[macro_export]
macro_rules! impl_from_for_source {
    ($($t:ty),*) => { $(
        impl From<$t> for Source<$t> {
            fn from(value: $t) -> Source<$t> {
                Source::Constant(value)
            }
        }
        impl From<&$t> for Source<$t> {
            fn from(value: &$t) -> Source<$t> {
                value.clone().into()
            }
        }
    )*
    };
}
impl_from_for_source!(u8, u16, u32, u64, u128, usize);
impl_from_for_source!(i8, i16, i32, i64, i128, isize);
impl_from_for_source!(f32, f64);
impl_from_for_source!(bool, char);
