use super::*;
use std::{iter::once, rc::Rc, sync::Arc};

#[derive(Clone)]
pub enum Source<T: 'static> {
    Constant(T),
    Obs(DynObs<T>),
}

impl<T: 'static> Source<T> {
    pub fn get(&self, cx: &mut BindContext) -> T
    where
        T: Copy,
    {
        match self {
            Self::Constant(value) => *value,
            Self::Obs(obs) => obs.get(cx),
        }
    }
    pub fn get_cloned(&self, cx: &mut BindContext) -> T
    where
        T: Clone,
    {
        match self {
            Self::Constant(value) => value.clone(),
            Self::Obs(obs) => obs.get(cx),
        }
    }
    pub fn with<U>(&self, f: impl FnOnce(&T, &mut BindContext) -> U, cx: &mut BindContext) -> U {
        match self {
            Self::Constant(value) => f(value, cx),
            Self::Obs(obs) => obs.with(|value, cx| f(value, cx), cx),
        }
    }

    pub fn head(self) -> T {
        match self {
            Self::Constant(value) => value,
            Self::Obs(obs) => obs.head(),
        }
    }
    pub fn head_cloned(&self) -> T
    where
        T: Clone,
    {
        match &self {
            Self::Constant(value) => value.clone(),
            Self::Obs(obs) => obs.head(),
        }
    }
    pub fn head_tail(self) -> (T, DynTail<T>) {
        BindScope::with(|scope| self.head_tail_with(scope))
    }
    pub fn head_tail_with(self, scope: &BindScope) -> (T, DynTail<T>) {
        match self {
            Source::Constant(x) => (x, DynTail::empty()),
            Source::Obs(obs) => obs.head_tail_with(scope),
        }
    }
    pub fn obs(self) -> Obs<impl Observable<Item = T>>
    where
        T: Copy,
    {
        self.into_obs()
    }
    pub fn obs_ref(self) -> ObsRef<impl ObservableRef<Item = T>> {
        self.into_obs_ref()
    }
    pub fn into_dyn(self) -> DynObs<T>
    where
        T: Copy,
    {
        match self {
            Source::Constant(value) => DynObs::constant(value),
            Source::Obs(o) => o.into_dyn_obs(),
        }
    }
    pub fn into_dyn_obs_ref(self) -> DynObsRef<T> {
        match self {
            Source::Constant(value) => DynObsRef::constant(value),
            Source::Obs(o) => o.as_ref().into_dyn_obs_ref(),
        }
    }
    pub fn map<U>(self, f: impl Fn(T) -> U + 'static) -> Source<U> {
        match self {
            Source::Constant(value) => Source::Constant(f(value)),
            Source::Obs(o) => Source::Obs(o.map(f)),
        }
    }

    pub fn cloned(self) -> Obs<impl Observable<Item = T>>
    where
        T: Clone,
    {
        obs(move |cx| self.get_cloned(cx))
    }

    pub fn fold<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, T) -> St + 'static,
    ) -> Fold<St> {
        match self {
            Source::Constant(x) => Fold::constant(f(initial_state, x)),
            Source::Obs(obs) => obs.fold(initial_state, f),
        }
    }
    pub fn collect_to<E: Extend<T> + 'static>(self, e: E) -> Fold<E> {
        match self {
            Source::Constant(x) => {
                let mut e = e;
                e.extend(once(x));
                Fold::constant(e)
            }
            Source::Obs(obs) => obs.collect_to(e),
        }
    }
    pub fn collect<E: Extend<T> + Default + 'static>(self) -> Fold<E> {
        self.collect_to(Default::default())
    }
    pub fn collect_vec(self) -> Fold<Vec<T>> {
        self.collect()
    }

    pub fn subscribe(self, f: impl FnMut(T) + 'static) -> Subscription {
        match self {
            Source::Constant(x) => {
                let mut f = f;
                f(x);
                Subscription::empty()
            }
            Source::Obs(obs) => obs.subscribe(f),
        }
    }
}
impl<T: Copy> Observable for Source<T> {
    type Item = T;

    fn get(&self, cx: &mut BindContext) -> Self::Item {
        Source::get(self, cx)
    }

    fn into_dyn_obs(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        Source::into_dyn(self)
    }
}
impl<T> ObservableRef for Source<T> {
    type Item = T;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        Source::with(self, f, cx)
    }

    fn into_dyn_obs_ref(self) -> DynObsRef<Self::Item>
    where
        Self: Sized,
    {
        Source::into_dyn_obs_ref(self)
    }
}

// pub trait IntoSource<T: 'static> {
//     fn into_source(self) -> Source<T>;
// }

// impl<T: 'static, S: Observable<Item = T>> IntoSource<T> for Obs<S> {
//     fn into_source(self) -> Source<T> {
//         self.into_dyn().into_source()
//     }
// }
// impl<T: 'static, S: Observable<Item = T> + Clone> IntoSource<T> for &Obs<S> {
//     fn into_source(self) -> Source<T> {
//         self.clone().into_source()
//     }
// }
// impl<T: Copy + 'static, S: ObservableBorrow<Item = T>> IntoSource<T> for ObsBorrow<S> {
//     fn into_source(self) -> Source<T> {
//         self.cloned().into_source()
//     }
// }
// impl<T: Copy + 'static, S: ObservableBorrow<Item = T> + Clone> IntoSource<T> for &ObsBorrow<S> {
//     fn into_source(self) -> Source<T> {
//         self.clone().into_source()
//     }
// }
// impl<T: Copy + 'static, S: ObservableRef<Item = T>> IntoSource<T> for ObsRef<S> {
//     fn into_source(self) -> Source<T> {
//         self.cloned().into_source()
//     }
// }
// impl<T: Copy + 'static, S: ObservableRef<Item = T> + Clone> IntoSource<T> for &ObsRef<S> {
//     fn into_source(self) -> Source<T> {
//         self.clone().into_source()
//     }
// }

// impl<T: 'static> IntoSource<T> for DynObs<T> {
//     fn into_source(self) -> Source<T> {
//         Source::Obs(self)
//     }
// }
// impl<T: 'static> IntoSource<T> for &DynObs<T> {
//     fn into_source(self) -> Source<T> {
//         self.clone().into_source()
//     }
// }
// impl<T: Copy + 'static> IntoSource<T> for DynObsBorrow<T> {
//     fn into_source(self) -> Source<T> {
//         Source::Obs(self.cloned())
//     }
// }
// impl<T: Copy + 'static> IntoSource<T> for &DynObsBorrow<T> {
//     fn into_source(self) -> Source<T> {
//         Source::Obs(self.cloned())
//     }
// }
// impl<T: Copy + 'static> IntoSource<T> for DynObsRef<T> {
//     fn into_source(self) -> Source<T> {
//         Source::Obs(self.cloned())
//     }
// }
// impl<T: Copy + 'static> IntoSource<T> for &DynObsRef<T> {
//     fn into_source(self) -> Source<T> {
//         Source::Obs(self.cloned())
//     }
// }
// impl<T> IntoSource<T> for Source<T> {
//     fn into_source(self) -> Source<T> {
//         self
//     }
// }
// impl<T: 'static> IntoSource<Rc<T>> for Rc<T> {
//     fn into_source(self) -> Source<Rc<T>> {
//         Source::Constant(self)
//     }
// }
// impl<T: 'static> IntoSource<Arc<T>> for Arc<T> {
//     fn into_source(self) -> Source<Arc<T>> {
//         Source::Constant(self)
//     }
// }
// impl<T: 'static> IntoSource<ObsCell<T>> for ObsCell<T> {
//     fn into_source(self) -> Source<ObsCell<T>> {
//         Source::Constant(self)
//     }
// }

// impl<T: IntoSource<T> + 'static> IntoSource<Option<T>> for Option<T> {
//     fn into_source(self) -> Source<Option<T>> {
//         Source::Constant(self)
//     }
// }
// impl<T: IntoSource<T> + 'static, E: IntoSource<E> + 'static> IntoSource<Result<T, E>>
//     for Result<T, E>
// {
//     fn into_source(self) -> Source<Result<T, E>> {
//         Source::Constant(self)
//     }
// }

// macro_rules! impl_into_source {
//     ($($t:ty),*) => { $(
//         impl IntoSource<$t> for $t {
//             fn into_source(self) -> Source<$t> {
//                 Source::Constant(self)
//             }
//         }
//     )*
//     };
// }

// impl_into_source!(u8, u16, u32, u64, u128, usize);
// impl_into_source!(i8, i16, i32, i64, i128, isize);
// impl_into_source!(f32, f64);
// impl_into_source!(bool, char);

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

impl<T, S> From<Obs<S>> for Source<T>
where
    S: Observable,
    S::Item: Into<T>,
{
    fn from(value: Obs<S>) -> Self {
        value.map_into::<T>().into_dyn().into()
    }
}
impl<T, S> From<&Obs<S>> for Source<T>
where
    S: Observable + Clone,
    S::Item: Into<T>,
{
    fn from(value: &Obs<S>) -> Self {
        value.clone().into()
    }
}
impl<T, S> From<ObsBorrow<S>> for Source<T>
where
    S: ObservableBorrow,
    S::Item: Into<T> + Copy,
{
    fn from(value: ObsBorrow<S>) -> Self {
        value.as_ref().into()
    }
}
impl<T, S> From<&ObsBorrow<S>> for Source<T>
where
    S: ObservableBorrow + Clone,
    S::Item: Into<T> + Copy,
{
    fn from(value: &ObsBorrow<S>) -> Self {
        value.clone().into()
    }
}
impl<T, S> From<ObsRef<S>> for Source<T>
where
    S: ObservableRef,
    S::Item: Into<T> + Copy,
{
    fn from(value: ObsRef<S>) -> Self {
        value.into_dyn().into()
    }
}
impl<T, S> From<&ObsRef<S>> for Source<T>
where
    S: ObservableRef + Clone,
    S::Item: Into<T> + Copy,
{
    fn from(value: &ObsRef<S>) -> Self {
        value.clone().into()
    }
}

impl<T, S: Into<T>> From<DynObs<S>> for Source<T> {
    fn from(value: DynObs<S>) -> Self {
        Source::Obs(value.map_into())
    }
}
impl<T, S: Into<T>> From<&DynObs<S>> for Source<T> {
    fn from(value: &DynObs<S>) -> Self {
        value.clone().into()
    }
}
impl<T, S> From<DynObsBorrow<S>> for Source<T>
where
    S: Into<T> + Copy,
{
    fn from(value: DynObsBorrow<S>) -> Self {
        value.as_ref().into()
    }
}
impl<T, S> From<&DynObsBorrow<S>> for Source<T>
where
    S: Into<T> + Copy,
{
    fn from(value: &DynObsBorrow<S>) -> Self {
        value.clone().into()
    }
}
impl<T, S> From<DynObsRef<S>> for Source<T>
where
    S: Into<T> + Copy,
{
    fn from(value: DynObsRef<S>) -> Self {
        value.cloned().into()
    }
}
impl<T, S> From<&DynObsRef<S>> for Source<T>
where
    S: Into<T> + Copy,
{
    fn from(value: &DynObsRef<S>) -> Self {
        value.clone().into()
    }
}

impl<T: Copy> From<&Source<T>> for Source<T> {
    fn from(value: &Source<T>) -> Self {
        value.clone()
    }
}
impl<T> From<Rc<T>> for Source<Rc<T>> {
    fn from(value: Rc<T>) -> Self {
        Source::Constant(value)
    }
}
impl<T> From<&Rc<T>> for Source<Rc<T>> {
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

impl<T: Into<Source<T>> + 'static> From<Option<T>> for Source<Option<T>> {
    fn from(value: Option<T>) -> Self {
        Source::Constant(value)
    }
}
impl<T, E> From<Result<T, E>> for Source<Result<T, E>>
where
    T: Into<Source<T>> + 'static,
    E: Into<Source<E>> + 'static,
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
