use crate::*;
use std::{borrow::Borrow, ops::Deref};

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub struct SourceBorrow<T: ?Sized + 'static>(pub DynObs<T>);

impl<T: ?Sized + 'static> Deref for SourceBorrow<T> {
    type Target = DynObs<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S: Observable, U> From<Obs<S>> for SourceBorrow<U>
where
    S: Observable,
    S::Item: Borrow<U>,
    U: ?Sized,
{
    fn from(s: Obs<S>) -> Self {
        s.map_borrow().into_dyn().into()
    }
}
impl<T, U> From<DynObs<T>> for SourceBorrow<U>
where
    T: ?Sized + 'static + Borrow<U>,
    U: ?Sized + 'static,
{
    fn from(s: DynObs<T>) -> Self {
        Self(s.map_borrow())
    }
}
impl<T, U> From<&DynObs<T>> for SourceBorrow<U>
where
    T: ?Sized + 'static + Borrow<U>,
    U: ?Sized + 'static,
{
    fn from(s: &DynObs<T>) -> Self {
        s.clone().into()
    }
}
impl<T, U> From<ObsCell<T>> for SourceBorrow<U>
where
    T: Borrow<U> + 'static,
    U: ?Sized + 'static,
{
    fn from(s: ObsCell<T>) -> Self {
        s.as_dyn().into()
    }
}
impl<T, U> From<&ObsCell<T>> for SourceBorrow<U>
where
    T: Borrow<U> + 'static,
    U: ?Sized + 'static,
{
    fn from(s: &ObsCell<T>) -> Self {
        s.as_dyn().into()
    }
}

impl<S, T> From<ObsCollector<S>> for SourceBorrow<T>
where
    S: Collect,
    S::Output: Borrow<T>,
    T: ?Sized,
{
    fn from(s: ObsCollector<S>) -> Self {
        (&s).into()
    }
}
impl<S, T> From<&ObsCollector<S>> for SourceBorrow<T>
where
    S: Collect,
    S::Output: Borrow<T>,
    T: ?Sized,
{
    fn from(s: &ObsCollector<S>) -> Self {
        s.obs().into()
    }
}
impl From<&'static str> for SourceBorrow<str> {
    fn from(s: &'static str) -> Self {
        DynObs::new_static(s).into()
    }
}
impl From<String> for SourceBorrow<str> {
    fn from(s: String) -> Self {
        DynObs::<str>::new_constant_map_ref(s, |s| s.as_str()).into()
    }
}
