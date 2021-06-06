use reactive_fn::observables::*;
use reactive_fn::*;

#[test]
fn define_into_obs_value() {}

#[derive(Clone, Copy)]
struct NewType(f64);

impl From<NewType> for f64 {
    fn from(value: NewType) -> Self {
        value.0
    }
}
impl From<f64> for NewType {
    fn from(value: f64) -> Self {
        Self(value)
    }
}

impl IntoObsValue<NewType> for NewType {
    type Observable = ConstantObservable<NewType>;
    fn into_obs_value(self) -> Obs<Self::Observable> {
        obs_constant(self)
    }
}
impl IntoObsValue<NewType> for f64 {
    type Observable = ConstantObservable<NewType>;
    fn into_obs_value(self) -> Obs<Self::Observable> {
        obs_constant(self.into())
    }
}
impl IntoObsValue<f64> for NewType {
    type Observable = ConstantObservable<f64>;
    fn into_obs_value(self) -> Obs<Self::Observable> {
        obs_constant(self.into())
    }
}
