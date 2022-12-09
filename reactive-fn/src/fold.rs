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
    pub(crate) fn constant(st: T) -> Self {
        Self(FoldData::Constant(st))
    }
    pub fn new(st: T, mut f: impl FnMut(&mut T, &mut ObsContext) + 'static) -> Self {
        Fold::from_dyn(Subscribe::new(Some(st), move |st, bc| {
            if let Some(st) = st {
                f(st, bc)
            }
        }))
    }

    pub fn stop(self) -> T {
        BindScope::with(move |scope| self.stop_with(scope))
    }
    pub fn stop_with(self, scope: &BindScope) -> T {
        match self.0 {
            FoldData::Constant(st) => st,
            FoldData::Dyn(this) => this.stop(scope),
        }
    }
}
impl<T: 'static> From<Fold<T>> for Subscription {
    fn from(x: Fold<T>) -> Self {
        match x.0 {
            FoldData::Constant(_) => Subscription::empty(),
            FoldData::Dyn(this) => Subscription(Some(this.as_dyn_any())),
        }
    }
}
