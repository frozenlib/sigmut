use super::*;
use std::{borrow::Borrow, ops::Deref};

pub struct SourceRef<T: ?Sized + 'static>(pub DynObsRef<T>);

impl<T: ?Sized + 'static> Deref for SourceRef<T> {
    type Target = DynObsRef<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub trait IntoSourceRef<T: ?Sized> {
    fn into_source_ref(self) -> SourceRef<T>;
}

impl<T, B> IntoSourceRef<T> for DynObs<B>
where
    T: ?Sized + 'static,
    B: Borrow<T>,
{
    fn into_source_ref(self) -> SourceRef<T> {
        (&self).into_source_ref()
    }
}
impl<T, B> IntoSourceRef<T> for &DynObs<B>
where
    T: ?Sized + 'static,
    B: Borrow<T>,
{
    fn into_source_ref(self) -> SourceRef<T> {
        SourceRef(self.as_ref().map_borrow())
    }
}

impl<T, B> IntoSourceRef<T> for DynObsRef<B>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn into_source_ref(self) -> SourceRef<T> {
        (&self).into_source_ref()
    }
}

impl<T, B> IntoSourceRef<T> for &DynObsRef<B>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn into_source_ref(self) -> SourceRef<T> {
        SourceRef(self.map_borrow())
    }
}
impl<T, B> IntoSourceRef<T> for DynObsBorrow<B>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn into_source_ref(self) -> SourceRef<T> {
        (&self).into_source_ref()
    }
}
impl<T, B> IntoSourceRef<T> for &DynObsBorrow<B>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn into_source_ref(self) -> SourceRef<T> {
        self.as_ref().into_source_ref()
    }
}

impl<S, T> IntoSourceRef<T> for Obs<S>
where
    S: Observable,
    S::Item: Borrow<T>,
    T: ?Sized,
{
    fn into_source_ref(self) -> SourceRef<T> {
        self.into_dyn_obs_ref().into_source_ref()
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
        self.into_dyn_obs_ref().into_source_ref()
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
        self.into_dyn_obs_ref().into_source_ref()
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
    B: Borrow<T> + 'static,
{
    fn into_source_ref(self) -> SourceRef<T> {
        self.obs().into_source_ref()
    }
}

impl<T, B> IntoSourceRef<T> for &ObsCell<B>
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
        DynObsRef::static_ref(self).into_source_ref()
    }
}
impl IntoSourceRef<str> for String {
    fn into_source_ref(self) -> SourceRef<str> {
        DynObsRef::<str>::constant_map(self, |s| &s).into_source_ref()
    }
}

///-------------------------

impl<T, B> From<DynObs<B>> for SourceRef<T>
where
    T: ?Sized + 'static,
    B: Borrow<T>,
{
    fn from(s: DynObs<B>) -> SourceRef<T> {
        (&s).into()
    }
}
impl<T, B> From<&DynObs<B>> for SourceRef<T>
where
    T: ?Sized + 'static,
    B: Borrow<T>,
{
    fn from(s: &DynObs<B>) -> SourceRef<T> {
        SourceRef(s.as_ref().map_borrow())
    }
}
impl<T, B> From<DynObsBorrow<B>> for SourceRef<T>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn from(s: DynObsBorrow<B>) -> SourceRef<T> {
        (&s).into()
    }
}
impl<T, B> From<&DynObsBorrow<B>> for SourceRef<T>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn from(s: &DynObsBorrow<B>) -> SourceRef<T> {
        s.as_ref().into()
    }
}
impl<T, B> From<DynObsRef<B>> for SourceRef<T>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn from(s: DynObsRef<B>) -> SourceRef<T> {
        (&s).into()
    }
}
impl<T, B> From<&DynObsRef<B>> for SourceRef<T>
where
    T: ?Sized + 'static,
    B: ?Sized + Borrow<T>,
{
    fn from(s: &DynObsRef<B>) -> SourceRef<T> {
        SourceRef(s.map_borrow())
    }
}

impl<S, T> From<Obs<S>> for SourceRef<T>
where
    S: Observable,
    S::Item: Borrow<T>,
    T: ?Sized + 'static,
{
    fn from(s: Obs<S>) -> Self {
        s.as_ref().into()
    }
}
impl<S, T> From<&Obs<S>> for SourceRef<T>
where
    S: Observable + Clone,
    S::Item: Borrow<T>,
    T: ?Sized + 'static,
{
    fn from(s: &Obs<S>) -> Self {
        s.clone().into()
    }
}

impl<S, T> From<ObsBorrow<S>> for SourceRef<T>
where
    S: ObservableBorrow,
    S::Item: Borrow<T>,
    T: ?Sized + 'static,
{
    fn from(s: ObsBorrow<S>) -> Self {
        s.as_ref().into()
    }
}
impl<S, T> From<&ObsBorrow<S>> for SourceRef<T>
where
    S: ObservableBorrow + Clone,
    S::Item: Borrow<T>,
    T: ?Sized + 'static,
{
    fn from(s: &ObsBorrow<S>) -> Self {
        s.clone().into()
    }
}
impl<S, T> From<ObsRef<S>> for SourceRef<T>
where
    S: ObservableRef,
    S::Item: Borrow<T>,
    T: ?Sized + 'static,
{
    fn from(s: ObsRef<S>) -> Self {
        SourceRef(s.into_dyn_obs_ref().map_borrow())
    }
}
impl<S, T> From<&ObsRef<S>> for SourceRef<T>
where
    S: ObservableRef + Clone,
    S::Item: Borrow<T>,
    T: ?Sized + 'static,
{
    fn from(s: &ObsRef<S>) -> Self {
        s.clone().into()
    }
}

impl<S, T> From<ObsCell<S>> for SourceRef<T>
where
    S: Borrow<T> + 'static,
    T: ?Sized + 'static,
{
    fn from(s: ObsCell<S>) -> Self {
        (&s).into()
    }
}
impl<S, T> From<&ObsCell<S>> for SourceRef<T>
where
    S: Borrow<T> + 'static,
    T: ?Sized + 'static,
{
    fn from(s: &ObsCell<S>) -> Self {
        s.obs().into()
    }
}
impl<S, T> From<ObsCollector<S>> for SourceRef<T>
where
    S: Collect,
    S::Output: Borrow<T>,
    T: ?Sized,
{
    fn from(s: ObsCollector<S>) -> Self {
        (&s).into()
    }
}
impl<S, T> From<&ObsCollector<S>> for SourceRef<T>
where
    S: Collect,
    S::Output: Borrow<T>,
    T: ?Sized,
{
    fn from(s: &ObsCollector<S>) -> Self {
        s.obs().into()
    }
}
impl From<&'static str> for SourceRef<str> {
    fn from(s: &'static str) -> Self {
        DynObsRef::static_ref(s).into()
    }
}
impl From<String> for SourceRef<str> {
    fn from(s: String) -> Self {
        DynObsRef::<str>::constant_map(s, |s| s.as_str()).into()
    }
}
