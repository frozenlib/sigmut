use reactive_fn::*;

#[test]
fn define_into_source() {}

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

impl IntoSource<NewType> for NewType {
    fn into_source(self) -> Source<NewType> {
        Source::Constant(self)
    }
}
impl IntoSource<NewType> for f64 {
    fn into_source(self) -> Source<NewType> {
        Source::Constant(self.into())
    }
}
impl IntoSource<f64> for NewType {
    fn into_source(self) -> Source<f64> {
        Source::Constant(self.into())
    }
}
