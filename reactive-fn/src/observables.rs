use crate::*;
use std::{
    any::{Any, TypeId},
    borrow::Borrow,
    marker::PhantomData,
    rc::Rc,
};

pub struct MapIntoObservable<S: Observable, T>(
    pub(crate) S,
    pub(crate) PhantomData<fn(S::Item) -> T>,
);

impl<S, T> Observable for MapIntoObservable<S, T>
where
    S: Observable + 'static,
    S::Item: Clone + Into<T>,
    T: 'static,
{
    type Item = T;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, bc: &mut ObsContext) -> U {
        self.0.with(|value, bc| f(&value.clone().into(), bc), bc)
    }
    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        into_dyn_by_convert(self, |s| s.0)
    }
}

pub struct MapBorrowObservable<S: Observable, T: ?Sized>(
    pub(crate) S,
    pub(crate) PhantomData<fn(&S::Item) -> &T>,
);

impl<S, T> Observable for MapBorrowObservable<S, T>
where
    S: Observable + 'static,
    S::Item: Borrow<T>,
    T: ?Sized + 'static,
{
    type Item = T;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, bc: &mut ObsContext) -> U {
        self.0.with(|value, bc| f(value.borrow(), bc), bc)
    }
    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        into_dyn_by_convert(self, |s| s.0)
    }
}

pub struct MapAsRefObservable<S: Observable, T: ?Sized>(
    pub(crate) S,
    pub(crate) PhantomData<fn(&S::Item) -> &T>,
);

impl<S, T> Observable for MapAsRefObservable<S, T>
where
    S: Observable + 'static,
    S::Item: AsRef<T>,
    T: ?Sized + 'static,
{
    type Item = T;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, bc: &mut ObsContext) -> U {
        self.0.with(|value, bc| f(value.as_ref(), bc), bc)
    }
    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        into_dyn_by_convert(self, |s| s.0)
    }
}

fn into_dyn_by_convert<Outer, Inner>(
    s: Outer,
    f: impl FnOnce(Outer) -> Inner,
) -> DynObs<Outer::Item>
where
    Outer: Observable + 'static,
    Inner: Observable + 'static,
{
    if TypeId::of::<Outer::Item>() == TypeId::of::<Inner>() {
        (*<dyn Any>::downcast_ref::<DynObs<Outer::Item>>(&f(s).into_dyn()).unwrap()).clone()
    } else {
        DynObs::new_dyn(Rc::new(s))
    }
}

pub struct ConstantObservable<T>(pub(crate) T);

impl<T: 'static> Observable for ConstantObservable<T> {
    type Item = T;

    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, bc: &mut ObsContext) -> U {
        f(&self.0, bc)
    }
    fn into_may(self) -> MayObs<Self::Item> {
        MayObs::Constant(self.0)
    }
}

pub struct StaticObservable<T: ?Sized + 'static>(pub(crate) &'static T);

impl<T: ?Sized> Observable for StaticObservable<T> {
    type Item = T;

    #[inline]
    fn with<U>(&self, f: impl FnOnce(&Self::Item, &mut ObsContext) -> U, bc: &mut ObsContext) -> U {
        f(self.0, bc)
    }
    #[inline]
    fn into_dyn(self) -> DynObs<Self::Item> {
        DynObs::new_static(self.0)
    }
}

// pub struct OptionObservable<S>(Option<S>);

// impl<S> OptionObservable<S>
// where
//     S: Observable,
//     S::Item: ToOwned,
// {
//     pub(crate) fn new(source: Option<S>) -> Self {
//         Self(source)
//     }
// }

// impl<S> Observable for OptionObservable<S>
// where
//     S: Observable + 'static,
//     S::Item: ToOwned,
// {
//     type Item = Option<<S::Item as ToOwned>::Owned>;

//     fn with<U>(
//         &self,
//         f: impl FnOnce(&Self::Item, &mut ObsContext) -> U,
//         bc: &mut ObsContext,
//     ) -> U {
//         if let Some(s) = &self.0 {
//             s.with(|value, bc| f(&Some(value.to_owned()), bc), bc)
//         } else {
//             f(&None, bc)
//         }
//     }
//     fn into_dyn(self) -> DynObs<Self::Item>
//     where
//         Self: Sized,
//     {
//         if let Some(s) = self.0 {
//             Obs(s).map(|value| Some(value.to_owned())).into_dyn()
//         } else {
//             DynObs::new_static(&None)
//         }
//     }
// }

// pub struct ResultObservable<S, E>(Result<S, Result<<S::Item as ToOwned>::Owned, E>>)
// where
//     S: Observable,
//     S::Item: ToOwned;

// impl<S, E> ResultObservable<S, E>
// where
//     S: Observable,
//     S::Item: ToOwned,
//     E: 'static,
// {
//     pub(crate) fn new(result: Result<S, E>) -> Self {
//         Self(match result {
//             Ok(s) => Ok(s),
//             Err(e) => Err(Err(e)),
//         })
//     }
// }

// impl<S, E> Observable for ResultObservable<S, E>
// where
//     S: Observable + 'static,
//     S::Item: ToOwned,
//     E: 'static,
// {
//     type Item = Result<<S::Item as ToOwned>::Owned, E>;

//     fn with<U>(
//         &self,
//         f: impl FnOnce(&Self::Item, &mut ObsContext) -> U,
//         bc: &mut ObsContext,
//     ) -> U {
//         match &self.0 {
//             Ok(s) => s.with(|value, bc| f(&Ok(value.to_owned()), bc), bc),
//             Err(e) => f(e, bc),
//         }
//     }
//     fn into_dyn(self) -> DynObs<Self::Item>
//     where
//         Self: Sized,
//     {
//         match self.0 {
//             Ok(s) => Obs(s).map(|value| Ok(value.to_owned())).into_dyn(),
//             Err(e) => obs_constant(e).into_dyn(),
//         }
//     }
// }
