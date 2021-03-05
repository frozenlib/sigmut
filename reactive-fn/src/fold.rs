use super::*;
use std::{any::Any, rc::Rc};

pub(crate) trait DynamicFold {
    type Output;

    fn stop(self: Rc<Self>, scope: &BindScope) -> Self::Output;
    fn as_dyn_any(self: Rc<Self>) -> Rc<dyn Any>;
}
pub struct Fold<T>(FoldData<T>);

enum FoldData<T> {
    Constant(T),
    Dyn(Rc<dyn DynamicFold<Output = T>>),
}

impl<T: 'static> Fold<T> {
    pub(crate) fn from_dyn(fold: Rc<dyn DynamicFold<Output = T>>) -> Self {
        Self(FoldData::Dyn(fold))
    }
    pub(crate) fn constant(value: T) -> Self {
        Self(FoldData::Constant(value))
    }
    pub fn new(value: T, mut f: impl FnMut(&mut T, &mut BindContext) + 'static) -> Self {
        match Subscribe::new(Some(value), move |value, cx| {
            if let Some(value) = value {
                f(value, cx)
            }
        }) {
            Ok(s) => Fold::from_dyn(s),
            Err(value) => Fold::constant(value.unwrap()),
        }
    }

    pub fn stop(self) -> T {
        BindScope::with(move |scope| self.stop_with(scope))
    }
    pub fn stop_with(self, scope: &BindScope) -> T {
        match self.0 {
            FoldData::Constant(value) => value,
            FoldData::Dyn(this) => this.stop(scope),
        }
    }
}
impl<T> From<Fold<T>> for Subscription {
    fn from(x: Fold<T>) -> Self {
        match x.0 {
            FoldData::Constant(_) => Subscription::empty(),
            FoldData::Dyn(this) => Subscription(Some(this.as_dyn_any())),
        }
    }
}
