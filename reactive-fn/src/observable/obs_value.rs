use super::{FromObservable, Obs, ObsBuilder, Observable, ObservableBuilder};
use crate::core::ObsContext;
use reactive_fn_macros::ObservableFmt;

#[derive(Clone, ObservableFmt)]
#[observable_fmt(self_crate, bound(T))]
pub enum ObsValue<T>
where
    T: 'static,
{
    Constant(T),
    Obs(Obs<T>),
}

impl<T> ObsValue<T>
where
    T: Clone + 'static,
{
    pub fn map<U>(self, f: impl Fn(T) -> U + 'static) -> ObsValue<U> {
        match self {
            ObsValue::Constant(value) => ObsValue::Constant(f(value)),
            ObsValue::Obs(o) => ObsValue::Obs(o.map_value(move |value| f(value.clone()))),
        }
    }
    pub fn obs_builder(self) -> ObsBuilder<impl ObservableBuilder<Item = T>> {
        ObsBuilder(FromObservable {
            o: self,
            into_obs: Self::into_obs,
        })
    }
    pub fn into_obs(self) -> Obs<T> {
        match self {
            ObsValue::Constant(value) => Obs::new_value(value),
            ObsValue::Obs(o) => o,
        }
    }
}
impl<T: 'static> Observable for ObsValue<T> {
    type Item = T;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, oc: &mut ObsContext) -> U {
        match self {
            Self::Constant(value) => f(value, oc),
            Self::Obs(obs) => obs.with(|value, oc| f(value, oc), oc),
        }
    }
}
