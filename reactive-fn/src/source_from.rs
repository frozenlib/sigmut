use crate::*;

pub trait SourceFrom<T>
where
    Self: Sized,
{
    fn source_from(_: T) -> Source<Self>;
}
pub trait IntoSource<T> {
    fn into_source(self) -> Source<T>;
}
impl<S, T: SourceFrom<S>> IntoSource<T> for S {
    fn into_source(self) -> Source<T> {
        T::source_from(self)
    }
}

impl<T, S> SourceFrom<Obs<S>> for T
where
    S: Observable,
    S::Item: Into<T>,
{
    fn source_from(value: Obs<S>) -> Source<Self> {
        value.map_into::<T>().into_dyn().into_source()
    }
}
impl<T, S> SourceFrom<&Obs<S>> for T
where
    S: Observable + Clone,
    S::Item: Into<T>,
{
    fn source_from(value: &Obs<S>) -> Source<Self> {
        value.clone().into_source()
    }
}
impl<T, S> SourceFrom<ObsBorrow<S>> for T
where
    S: ObservableBorrow,
    S::Item: Sized,
    for<'a> &'a S::Item: Into<T>,
{
    fn source_from(value: ObsBorrow<S>) -> Source<Self> {
        value.as_ref().into_source()
    }
}
impl<T, S> SourceFrom<&ObsBorrow<S>> for T
where
    S: ObservableBorrow + Clone,
    S::Item: Sized,
    for<'a> &'a S::Item: Into<T>,
{
    fn source_from(value: &ObsBorrow<S>) -> Source<Self> {
        value.clone().into_source()
    }
}
impl<T, S> SourceFrom<ObsRef<S>> for T
where
    S: ObservableRef,
    S::Item: Sized,
    for<'a> &'a S::Item: Into<T>,
{
    fn source_from(value: ObsRef<S>) -> Source<Self> {
        value.into_dyn().into_source()
    }
}
impl<T, S> SourceFrom<&ObsRef<S>> for T
where
    S: ObservableRef + Clone,
    S::Item: Sized,
    for<'a> &'a S::Item: Into<T>,
{
    fn source_from(value: &ObsRef<S>) -> Source<Self> {
        value.clone().into_source()
    }
}
impl<T, S: Into<T>> SourceFrom<DynObs<S>> for T {
    fn source_from(value: DynObs<S>) -> Source<Self> {
        Source::Obs(value.map_into())
    }
}
impl<T, S: Into<T>> SourceFrom<&DynObs<S>> for T {
    fn source_from(value: &DynObs<S>) -> Source<Self> {
        value.clone().into_source()
    }
}
impl<T, S> SourceFrom<DynObsBorrow<S>> for T
where
    for<'a> &'a S: Into<T>,
{
    fn source_from(value: DynObsBorrow<S>) -> Source<Self> {
        value.as_ref().into_source()
    }
}
impl<T, S> SourceFrom<&DynObsBorrow<S>> for T
where
    for<'a> &'a S: Into<T>,
{
    fn source_from(value: &DynObsBorrow<S>) -> Source<Self> {
        value.clone().into_source()
    }
}
impl<T, S> SourceFrom<DynObsRef<S>> for T
where
    for<'a> &'a S: Into<T>,
{
    fn source_from(value: DynObsRef<S>) -> Source<Self> {
        value.map(|x| x.into()).into_source()
    }
}
impl<T, S> SourceFrom<&DynObsRef<S>> for T
where
    for<'a> &'a S: Into<T>,
{
    fn source_from(value: &DynObsRef<S>) -> Source<Self> {
        value.clone().into_source()
    }
}
impl<T: Into<U>, U> SourceFrom<Source<T>> for U {
    fn source_from(value: Source<T>) -> Source<Self> {
        value.map(|x| x.into())
    }
}
impl<T: Copy + Into<U>, U> SourceFrom<&Source<T>> for U {
    fn source_from(value: &Source<T>) -> Source<Self> {
        value.clone().into_source()
    }
}

impl<T> SourceFrom<ObsCell<T>> for ObsCell<T> {
    fn source_from(value: ObsCell<T>) -> Source<Self> {
        Source::Constant(value)
    }
}
impl<T> SourceFrom<&ObsCell<T>> for ObsCell<T> {
    fn source_from(value: &ObsCell<T>) -> Source<Self> {
        value.clone().into_source()
    }
}
impl<T> SourceFrom<ObsCell<T>> for T
where
    for<'a> &'a T: Into<T>,
{
    fn source_from(value: ObsCell<T>) -> Source<Self> {
        value.obs().into_source()
    }
}
impl<T> SourceFrom<&ObsCell<T>> for T
where
    for<'a> &'a T: Into<T>,
{
    fn source_from(value: &ObsCell<T>) -> Source<Self> {
        value.clone().into_source()
    }
}

#[macro_export]
macro_rules! impl_source_from_for {
    ($t:ty) => {
        impl<T: Into<$t> + Clone> From<&T> for $t {
            fn from(value: &T) -> Self {
                value.clone().into()
            }
        }
        impl<T: Into<$t>> SourceFrom<T> for $t {
            fn source_from(value: T) -> Source<Self> {
                Source::Constant(value.into())
            }
        }
    };
}
