use super::{Obs, ObsBuilder, Observable, ObservableBuilder};
use crate::core::ObsContext;
use reactive_fn_macros::ObservableFmt;
use std::borrow::Borrow;

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
    pub fn obs_builder(self) -> ObsBuilder<Self> {
        ObsBuilder(self)
    }
}
impl<T> Observable for ObsValue<T>
where
    T: 'static,
{
    type Item = T;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, oc: &mut ObsContext) -> U {
        match self {
            Self::Constant(value) => f(value.borrow(), oc),
            Self::Obs(obs) => obs.with(|value, oc| f(value, oc), oc),
        }
    }
}

impl<T> ObservableBuilder for ObsValue<T>
where
    T: 'static,
{
    type Item = T;
    type Observable = Self;

    fn build_observable(self) -> Self::Observable {
        self
    }

    fn build_obs(self) -> Obs<Self::Item> {
        match self {
            ObsValue::Constant(value) => Obs::from_value(value),
            ObsValue::Obs(o) => o,
        }
    }
}
