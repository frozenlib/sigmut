use super::*;
use std::borrow::Borrow;

#[derive(Clone)]
pub enum MayObs<T>
where
    T: 'static,
{
    Constant(T),
    Obs(Obs<T>),
}

impl<T> MayObs<T>
where
    T: Clone + 'static,
{
    pub fn obs(&self) -> ImplObs<impl Observable<Item = T>> {
        ImplObs(self.clone())
    }

    pub fn map<U>(self, f: impl Fn(T) -> U + 'static) -> MayObs<U> {
        match self {
            MayObs::Constant(value) => MayObs::Constant(f(value)),
            MayObs::Obs(o) => MayObs::Obs(o.map(move |value| f(value.clone()))),
        }
    }
}
impl<T> Observable for MayObs<T>
where
    T: 'static,
{
    type Item = T;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, bc: &mut ObsContext) -> U {
        match self {
            Self::Constant(value) => f(value.borrow(), bc),
            Self::Obs(obs) => obs.with(|value, bc| f(value, bc), bc),
        }
    }
    fn into_dyn(self) -> Obs<Self::Item>
    where
        Self: Sized,
    {
        match self {
            MayObs::Constant(value) => obs_constant(value).into_dyn(),
            MayObs::Obs(o) => o.into_dyn(),
        }
    }
    fn into_may(self) -> MayObs<Self::Item> {
        self
    }
}
