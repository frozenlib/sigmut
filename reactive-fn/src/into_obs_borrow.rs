use crate::observables::*;
use crate::*;
use std::borrow::Borrow;

pub trait IntoObsBorrow<T: ?Sized> {
    type Observable: Observable<Item = T>;
    fn into_obs_borrow(self) -> Obs<Self::Observable>;
}

impl<S: Observable, U> IntoObsBorrow<U> for Obs<S>
where
    S: Observable,
    S::Item: Borrow<U>,
    U: 'static + ?Sized,
{
    type Observable = MapBorrowObservable<S, U>;

    fn into_obs_borrow(self) -> Obs<Self::Observable> {
        self.map_borrow()
    }
}
impl<T, U> IntoObsBorrow<U> for DynObs<T>
where
    T: 'static + ?Sized + Borrow<U>,
    U: 'static + ?Sized,
{
    type Observable = MapBorrowObservable<DynObs<T>, U>;

    fn into_obs_borrow(self) -> Obs<Self::Observable> {
        self.obs().map_borrow()
    }
}
impl<T, U> IntoObsBorrow<U> for &DynObs<T>
where
    T: 'static + ?Sized + Borrow<U>,
    U: 'static + ?Sized,
{
    type Observable = MapBorrowObservable<DynObs<T>, U>;

    fn into_obs_borrow(self) -> Obs<Self::Observable> {
        self.obs().map_borrow()
    }
}

impl<T, U> IntoObsBorrow<U> for ObsCell<T>
where
    T: 'static + Borrow<U>,
    U: 'static + ?Sized,
{
    type Observable = MapBorrowObservable<ObsCell<T>, U>;

    fn into_obs_borrow(self) -> Obs<Self::Observable> {
        self.obs().map_borrow()
    }
}
impl<T, U> IntoObsBorrow<U> for &ObsCell<T>
where
    T: 'static + Borrow<U>,
    U: 'static + ?Sized,
{
    type Observable = MapBorrowObservable<ObsCell<T>, U>;

    fn into_obs_borrow(self) -> Obs<Self::Observable> {
        self.obs().map_borrow()
    }
}

impl<C, T> IntoObsBorrow<T> for ObsCollector<C>
where
    C: Collect,
    C::Output: Borrow<T>,
    T: 'static + ?Sized,
{
    type Observable = MapBorrowObservable<ObsCollector<C>, T>;
    fn into_obs_borrow(self) -> Obs<Self::Observable> {
        self.obs().into_obs_borrow()
    }
}
impl<C, T> IntoObsBorrow<T> for &ObsCollector<C>
where
    C: Collect,
    C::Output: Borrow<T>,
    T: 'static + ?Sized,
{
    type Observable = MapBorrowObservable<ObsCollector<C>, T>;
    fn into_obs_borrow(self) -> Obs<Self::Observable> {
        self.obs().into_obs_borrow()
    }
}

impl IntoObsBorrow<str> for &'static str {
    type Observable = StaticObservable<str>;
    fn into_obs_borrow(self) -> Obs<Self::Observable> {
        obs_static(self)
    }
}
impl IntoObsBorrow<str> for String {
    type Observable = MapBorrowObservable<ConstantObservable<String>, str>;
    fn into_obs_borrow(self) -> Obs<Self::Observable> {
        obs_constant(self).map_borrow()
    }
}
