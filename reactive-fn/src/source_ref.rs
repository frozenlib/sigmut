use super::*;
use std::borrow::Borrow;

pub type SourceRef<T> = DynObsRef<T>;
pub trait IntoSourceRef<T: ?Sized> {
    fn into_source_ref(self) -> SourceRef<T>;
}

impl<T, B> IntoSourceRef<T> for DynObs<B>
where
    T: ?Sized + 'static,
    B: Borrow<T>,
{
    fn into_source_ref(self) -> SourceRef<T> {
        self.as_ref().map_borrow()
    }
}
impl<T, B> IntoSourceRef<T> for &DynObs<B>
where
    T: ?Sized + 'static,
    B: Borrow<T>,
{
    fn into_source_ref(self) -> SourceRef<T> {
        self.as_ref().map_borrow()
    }
}

impl<T, B> IntoSourceRef<T> for DynObsRef<B>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn into_source_ref(self) -> SourceRef<T> {
        self.map_borrow()
    }
}

impl<T, B> IntoSourceRef<T> for &DynObsRef<B>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn into_source_ref(self) -> SourceRef<T> {
        self.map_borrow()
    }
}
impl<T, B> IntoSourceRef<T> for DynObsBorrow<B>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn into_source_ref(self) -> SourceRef<T> {
        self.as_ref().map_borrow()
    }
}
impl<T, B> IntoSourceRef<T> for &DynObsBorrow<B>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn into_source_ref(self) -> SourceRef<T> {
        self.as_ref().map_borrow()
    }
}

impl<S, T> IntoSourceRef<T> for Obs<S>
where
    S: Observable,
    S::Item: Borrow<T>,
    T: ?Sized,
{
    fn into_source_ref(self) -> SourceRef<T> {
        Obs::into_dyn(self).map_borrow()
    }
}
impl<S, T> IntoSourceRef<T> for &Obs<S>
where
    S: Observable + Clone,
    S::Item: Borrow<T>,
    T: ?Sized,
{
    fn into_source_ref(self) -> SourceRef<T> {
        self.clone().into_source_ref()
    }
}

impl<S, T> IntoSourceRef<T> for ObsBorrow<S>
where
    S: ObservableBorrow,
    S::Item: Borrow<T>,
    T: ?Sized,
{
    fn into_source_ref(self) -> SourceRef<T> {
        ObsBorrow::into_dyn(self).as_ref().map_borrow()
    }
}
impl<S, T> IntoSourceRef<T> for &ObsBorrow<S>
where
    S: ObservableBorrow + Clone,
    S::Item: Borrow<T>,
    T: ?Sized,
{
    fn into_source_ref(self) -> SourceRef<T> {
        self.clone().into_source_ref()
    }
}

impl<S, T> IntoSourceRef<T> for ObsRef<S>
where
    S: ObservableRef,
    S::Item: Borrow<T>,
    T: ?Sized,
{
    fn into_source_ref(self) -> SourceRef<T> {
        ObsRef::into_dyn(self).map_borrow()
    }
}
impl<S, T> IntoSourceRef<T> for &ObsRef<S>
where
    S: ObservableRef + Clone,
    S::Item: Borrow<T>,
    T: ?Sized,
{
    fn into_source_ref(self) -> SourceRef<T> {
        self.clone().into_source_ref()
    }
}

impl<T, B> IntoSourceRef<T> for ObsCell<B>
where
    T: ?Sized,
    B: Borrow<T> + Copy + 'static,
{
    fn into_source_ref(self) -> SourceRef<T> {
        self.obs().into_source_ref()
    }
}

impl<T, B> IntoSourceRef<T> for &ObsCell<B>
where
    T: ?Sized,
    B: Borrow<T> + Copy + 'static,
{
    fn into_source_ref(self) -> SourceRef<T> {
        self.obs().into_source_ref()
    }
}

impl<T, B> IntoSourceRef<T> for ObsRefCell<B>
where
    T: ?Sized,
    B: Borrow<T> + 'static,
{
    fn into_source_ref(self) -> SourceRef<T> {
        self.obs().into_source_ref()
    }
}

impl<T, B> IntoSourceRef<T> for &ObsRefCell<B>
where
    T: ?Sized,
    B: Borrow<T> + 'static,
{
    fn into_source_ref(self) -> SourceRef<T> {
        self.obs().into_source_ref()
    }
}

impl<S, T> IntoSourceRef<T> for ObsCollector<S>
where
    S: Collect,
    S::Output: Borrow<T>,
    T: ?Sized,
{
    fn into_source_ref(self) -> SourceRef<T> {
        self.obs().into_source_ref()
    }
}
impl<S, T> IntoSourceRef<T> for &ObsCollector<S>
where
    S: Collect,
    S::Output: Borrow<T>,
    T: ?Sized,
{
    fn into_source_ref(self) -> SourceRef<T> {
        self.obs().into_source_ref()
    }
}

impl IntoSourceRef<str> for &'static str {
    fn into_source_ref(self) -> SourceRef<str> {
        DynObsRef::static_ref(self)
    }
}
impl IntoSourceRef<str> for String {
    fn into_source_ref(self) -> SourceRef<str> {
        DynObsRef::<str>::constant_map(self, |s| &s)
    }
}
