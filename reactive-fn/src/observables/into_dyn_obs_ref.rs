use crate::*;
use std::borrow::Borrow;
pub trait IntoDynObsRef<T: ?Sized> {
    fn into_dyn_obs_ref(self) -> DynObsRef<T>;
}

impl<T, B> IntoDynObsRef<T> for DynObs<B>
where
    T: ?Sized + 'static,
    B: Borrow<T>,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.as_ref().map_borrow()
    }
}
impl<T, B> IntoDynObsRef<T> for &DynObs<B>
where
    T: ?Sized + 'static,
    B: Borrow<T>,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.as_ref().map_borrow()
    }
}

impl<T, B> IntoDynObsRef<T> for DynObsRef<B>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.map_borrow()
    }
}

impl<T, B> IntoDynObsRef<T> for &DynObsRef<B>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.map_borrow()
    }
}
impl<T, B> IntoDynObsRef<T> for DynObsBorrow<B>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.as_ref().map_borrow()
    }
}
impl<T, B> IntoDynObsRef<T> for &DynObsBorrow<B>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.as_ref().map_borrow()
    }
}

impl<S: Observable> IntoDynObsRef<S::Item> for Obs<S> {
    fn into_dyn_obs_ref(self) -> DynObsRef<S::Item> {
        Obs::into_dyn_ref(self)
    }
}
impl<S: ObservableBorrow> IntoDynObsRef<S::Item> for ObsBorrow<S> {
    fn into_dyn_obs_ref(self) -> DynObsRef<S::Item> {
        ObsBorrow::into_dyn_ref(self)
    }
}
impl<S: ObservableRef> IntoDynObsRef<S::Item> for ObsRef<S> {
    fn into_dyn_obs_ref(self) -> DynObsRef<S::Item> {
        ObsRef::into_dyn(self)
    }
}
