use crate::observables::*;
use crate::*;

pub trait IntoObsValue<T> {
    type Observable: Observable<Item = T>;
    fn into_obs_value(self) -> Obs<Self::Observable>;

    fn into_source(self) -> Source<T>
    where
        Self: Sized,
    {
        self.into_obs_value().into_source()
    }
}

impl<S: Observable, U> IntoObsValue<U> for Obs<S>
where
    S::Item: Clone + Into<U>,
    U: 'static,
{
    type Observable = MapIntoObservable<S, U>;
    fn into_obs_value(self) -> Obs<Self::Observable> {
        self.map_into()
    }
}
impl<T, U> IntoObsValue<U> for DynObs<T>
where
    T: Clone + Into<U>,
    U: 'static,
{
    type Observable = MapIntoObservable<DynObs<T>, U>;
    fn into_obs_value(self) -> Obs<Self::Observable> {
        self.obs().map_into()
    }
}
impl<T, U> IntoObsValue<U> for &DynObs<T>
where
    T: Clone + Into<U>,
    U: 'static,
{
    type Observable = MapIntoObservable<DynObs<T>, U>;
    fn into_obs_value(self) -> Obs<Self::Observable> {
        self.obs().map_into()
    }
}

// `IntoObsValue<U> for ObsCell<T: Into<U>>` is not implemented because it conflicts with `IntoObsValue<ObsCell<T>> for ObsCell<T>`.
// `IntoObsValue<T> for ObsCell<T>` could be implemented, but it's not.

impl<T> IntoObsValue<ObsCell<T>> for ObsCell<T> {
    type Observable = ConstantObservable<ObsCell<T>>;
    fn into_obs_value(self) -> Obs<Self::Observable> {
        obs_constant(self)
    }
}
impl<T> IntoObsValue<ObsCell<T>> for &ObsCell<T> {
    type Observable = ConstantObservable<ObsCell<T>>;
    fn into_obs_value(self) -> Obs<Self::Observable> {
        obs_constant(self.clone())
    }
}

impl<S, T> IntoObsValue<Option<T>> for Option<S>
where
    S: IntoObsValue<T>,
    T: Clone,
{
    type Observable = OptionObservable<S::Observable>;
    fn into_obs_value(self) -> Obs<Self::Observable> {
        Obs(OptionObservable::new(self.map(|s| s.into_obs_value().0)))
    }
}
impl<S, T, E0, E1> IntoObsValue<Result<T, E1>> for Result<S, E0>
where
    S: IntoObsValue<T>,
    T: Clone,
    E0: Into<E1>,
    E1: Clone + 'static,
{
    type Observable = ResultObservable<S::Observable, E1>;
    fn into_obs_value(self) -> Obs<Self::Observable> {
        Obs(ResultObservable::new(match self {
            Ok(s) => Ok(s.into_obs_value().0),
            Err(e) => Err(e.into()),
        }))
    }
}

macro_rules! impl_into_obs_value {
    ($($t:ty),*) => { $(
        impl IntoObsValue<$t> for $t {
            type Observable = ConstantObservable<$t>;
            fn into_obs_value(self) -> Obs<Self::Observable> {
                obs_constant(self)
            }
        }
    )*
    };
}
impl_into_obs_value!(u8, u16, u32, u64, u128, usize);
impl_into_obs_value!(i8, i16, i32, i64, i128, isize);
impl_into_obs_value!(f32, f64);
impl_into_obs_value!(bool, char);
