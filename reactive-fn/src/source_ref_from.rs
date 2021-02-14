use crate::*;

pub trait SourceRefFrom<T> {
    fn source_ref_from(_: T) -> SourceRef<Self>;
}
pub trait IntoSourceRef<T: ?Sized> {
    fn into_source_ref(self) -> SourceRef<T>;
}
impl<S, T: SourceRefFrom<S> + ?Sized> IntoSourceRef<T> for S {
    fn into_source_ref(self) -> SourceRef<T> {
        T::source_ref_from(self)
    }
}

impl<T, B> SourceRefFrom<SourceRef<B>> for T
where
    T: ?Sized + 'static,
    B: AsRef<T>,
{
    fn source_ref_from(s: SourceRef<B>) -> SourceRef<Self> {
        (&s).into_source_ref()
    }
}
impl<T, B> SourceRefFrom<&SourceRef<B>> for T
where
    T: ?Sized + 'static,
    B: AsRef<T>,
{
    fn source_ref_from(s: &SourceRef<B>) -> SourceRef<Self> {
        (&s.0).into_source_ref()
    }
}

impl<T, B> SourceRefFrom<DynObs<B>> for T
where
    T: ?Sized + 'static,
    B: AsRef<T>,
{
    fn source_ref_from(s: DynObs<B>) -> SourceRef<T> {
        (&s).into_source_ref()
    }
}
impl<T, B> SourceRefFrom<&DynObs<B>> for T
where
    T: ?Sized + 'static,
    B: AsRef<T>,
{
    fn source_ref_from(s: &DynObs<B>) -> SourceRef<T> {
        SourceRef(s.as_ref().map_as_ref())
    }
}
impl<T, B> SourceRefFrom<DynObsBorrow<B>> for T
where
    T: ?Sized + 'static,
    B: ?Sized + AsRef<T>,
{
    fn source_ref_from(s: DynObsBorrow<B>) -> SourceRef<T> {
        (&s).into_source_ref()
    }
}
impl<T, B> SourceRefFrom<&DynObsBorrow<B>> for T
where
    T: ?Sized + 'static,
    B: ?Sized + AsRef<T>,
{
    fn source_ref_from(s: &DynObsBorrow<B>) -> SourceRef<T> {
        s.as_ref().into_source_ref()
    }
}
impl<T, B> SourceRefFrom<DynObsRef<B>> for T
where
    T: ?Sized + 'static,
    B: ?Sized + AsRef<T>,
{
    fn source_ref_from(s: DynObsRef<B>) -> SourceRef<T> {
        (&s).into_source_ref()
    }
}
impl<T, B> SourceRefFrom<&DynObsRef<B>> for T
where
    T: ?Sized + 'static,
    B: ?Sized + AsRef<T>,
{
    fn source_ref_from(s: &DynObsRef<B>) -> SourceRef<T> {
        SourceRef(s.map_as_ref())
    }
}

impl<S, T> SourceRefFrom<Obs<S>> for T
where
    S: Observable,
    S::Item: AsRef<T>,
    T: ?Sized + 'static,
{
    fn source_ref_from(s: Obs<S>) -> SourceRef<Self> {
        s.as_ref().into_source_ref()
    }
}
impl<S, T> SourceRefFrom<&Obs<S>> for T
where
    S: Observable + Clone,
    S::Item: AsRef<T>,
    T: ?Sized + 'static,
{
    fn source_ref_from(s: &Obs<S>) -> SourceRef<Self> {
        s.clone().into_source_ref()
    }
}

impl<S, T> SourceRefFrom<ObsBorrow<S>> for T
where
    S: ObservableBorrow,
    S::Item: AsRef<T>,
    T: ?Sized + 'static,
{
    fn source_ref_from(s: ObsBorrow<S>) -> SourceRef<Self> {
        s.as_ref().into_source_ref()
    }
}
impl<S, T> SourceRefFrom<&ObsBorrow<S>> for T
where
    S: ObservableBorrow + Clone,
    S::Item: AsRef<T>,
    T: ?Sized + 'static,
{
    fn source_ref_from(s: &ObsBorrow<S>) -> SourceRef<Self> {
        s.clone().into_source_ref()
    }
}
impl<S, T> SourceRefFrom<ObsRef<S>> for T
where
    S: ObservableRef,
    S::Item: AsRef<T>,
    T: ?Sized + 'static,
{
    fn source_ref_from(s: ObsRef<S>) -> SourceRef<Self> {
        SourceRef(s.into_dyn_obs_ref().map_as_ref())
    }
}
impl<S, T> SourceRefFrom<&ObsRef<S>> for T
where
    S: ObservableRef + Clone,
    S::Item: AsRef<T>,
    T: ?Sized + 'static,
{
    fn source_ref_from(s: &ObsRef<S>) -> SourceRef<Self> {
        s.clone().into_source_ref()
    }
}
