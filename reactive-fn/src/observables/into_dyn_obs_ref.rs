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

impl<S, T> IntoDynObsRef<T> for Obs<S>
where
    S: Observable,
    S::Item: Borrow<T>,
    T: ?Sized,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        Obs::into_dyn_ref(self).map_borrow()
    }
}
impl<S, T> IntoDynObsRef<T> for &Obs<S>
where
    S: Observable + Clone,
    S::Item: Borrow<T>,
    T: ?Sized,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.clone().into_dyn_obs_ref()
    }
}

impl<S, T> IntoDynObsRef<T> for ObsBorrow<S>
where
    S: ObservableBorrow,
    S::Item: Borrow<T>,
    T: ?Sized,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        ObsBorrow::into_dyn_ref(self).map_borrow()
    }
}
impl<S, T> IntoDynObsRef<T> for &ObsBorrow<S>
where
    S: ObservableBorrow + Clone,
    S::Item: Borrow<T>,
    T: ?Sized,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.clone().into_dyn_obs_ref()
    }
}

impl<S, T> IntoDynObsRef<T> for ObsRef<S>
where
    S: ObservableRef,
    S::Item: Borrow<T>,
    T: ?Sized,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        ObsRef::into_dyn(self).map_borrow()
    }
}
impl<S, T> IntoDynObsRef<T> for &ObsRef<S>
where
    S: ObservableRef + Clone,
    S::Item: Borrow<T>,
    T: ?Sized,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.clone().into_dyn_obs_ref()
    }
}

impl<T, B> IntoDynObsRef<T> for ObsCell<B>
where
    T: ?Sized,
    B: Borrow<T> + Copy + 'static,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.obs().into_dyn_obs_ref()
    }
}

impl<T, B> IntoDynObsRef<T> for &ObsCell<B>
where
    T: ?Sized,
    B: Borrow<T> + Copy + 'static,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.obs().into_dyn_obs_ref()
    }
}

impl<T, B> IntoDynObsRef<T> for ObsRefCell<B>
where
    T: ?Sized,
    B: Borrow<T> + 'static,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.obs().into_dyn_obs_ref()
    }
}

impl<T, B> IntoDynObsRef<T> for &ObsRefCell<B>
where
    T: ?Sized,
    B: Borrow<T> + 'static,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.obs().into_dyn_obs_ref()
    }
}

impl<S, T> IntoDynObsRef<T> for ObsCollector<S>
where
    S: Collect,
    S::Output: Borrow<T>,
    T: ?Sized,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.obs().into_dyn_obs_ref()
    }
}
impl<S, T> IntoDynObsRef<T> for &ObsCollector<S>
where
    S: Collect,
    S::Output: Borrow<T>,
    T: ?Sized,
{
    fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        self.obs().into_dyn_obs_ref()
    }
}
