use crate::*;
use std::borrow::Borrow;

pub type SourceBorrow<T> = DynObs<T>;

pub trait IntoSourceBorrow<T: ?Sized> {
    fn into_source_borrow(self) -> SourceBorrow<T>;
}

impl<S: Observable, U> IntoSourceBorrow<U> for Obs<S>
where
    S: Observable,
    S::Item: Borrow<U>,
    U: ?Sized,
{
    fn into_source_borrow(self) -> SourceBorrow<U> {
        self.map_borrow().into_dyn().into_source_borrow()
    }
}
impl<T, U> IntoSourceBorrow<U> for DynObs<T>
where
    T: ?Sized + 'static + Borrow<U>,
    U: ?Sized + 'static,
{
    fn into_source_borrow(self) -> SourceBorrow<U> {
        self.map_borrow()
    }
}
impl<T, U> IntoSourceBorrow<U> for &DynObs<T>
where
    T: ?Sized + 'static + Borrow<U>,
    U: ?Sized + 'static,
{
    fn into_source_borrow(self) -> SourceBorrow<U> {
        self.clone().into_source_borrow()
    }
}
impl<T, U> IntoSourceBorrow<U> for ObsCell<T>
where
    T: 'static + Borrow<U>,
    U: ?Sized + 'static,
{
    fn into_source_borrow(self) -> SourceBorrow<U> {
        self.obs().into_source_borrow()
    }
}
impl<T, U> IntoSourceBorrow<U> for &ObsCell<T>
where
    T: 'static + Borrow<U>,
    U: ?Sized + 'static,
{
    fn into_source_borrow(self) -> SourceBorrow<U> {
        self.obs().into_source_borrow()
    }
}

impl<S, T> IntoSourceBorrow<T> for ObsCollector<S>
where
    S: Collect,
    S::Output: Borrow<T>,
    T: ?Sized,
{
    fn into_source_borrow(self) -> SourceBorrow<T> {
        self.obs().into_source_borrow()
    }
}
impl<S, T> IntoSourceBorrow<T> for &ObsCollector<S>
where
    S: Collect,
    S::Output: Borrow<T>,
    T: ?Sized,
{
    fn into_source_borrow(self) -> SourceBorrow<T> {
        self.obs().into_source_borrow()
    }
}
impl IntoSourceBorrow<str> for &'static str {
    fn into_source_borrow(self) -> SourceBorrow<str> {
        DynObs::new_static(self).into_source_borrow()
    }
}
impl IntoSourceBorrow<str> for String {
    fn into_source_borrow(self) -> SourceBorrow<str> {
        DynObs::new_constant_map_ref(self, |s| s.as_str()).into_source_borrow()
    }
}
