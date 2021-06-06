use crate::*;
use std::{
    any::{Any, TypeId},
    marker::PhantomData,
    rc::Rc,
};

pub struct MapIntoObservable<S: Observable, T>(S, PhantomData<fn(S::Item) -> T>);

impl<S, T> MapIntoObservable<S, T>
where
    S: Observable,
    S::Item: Clone + Into<T>,
    T: 'static,
{
    pub(crate) fn new(source: S) -> Self {
        Self(source, PhantomData)
    }
}

impl<S, T> Observable for MapIntoObservable<S, T>
where
    S: Observable,
    S::Item: Clone + Into<T>,
    T: 'static,
{
    type Item = T;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        self.0.with(|value, cx| f(&value.clone().into(), cx), cx)
    }
    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        if TypeId::of::<S::Item>() == TypeId::of::<T>() {
            <dyn Any>::downcast_ref::<DynObs<T>>(&self.0.into_dyn())
                .unwrap()
                .clone()
        } else {
            DynObs::new_dyn(Rc::new(DynamicObs(self)))
        }
    }
}

pub struct ConstantObservable<T>(T);

impl<T: 'static> ConstantObservable<T> {
    pub(crate) fn new(value: T) -> Self {
        Self(value)
    }
}

impl<T: 'static> Observable for ConstantObservable<T> {
    type Item = T;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        f(&self.0, cx)
    }
}

pub struct OptionObservable<S>(Option<S>);

impl<S> OptionObservable<S>
where
    S: Observable,
    S::Item: ToOwned,
{
    pub(crate) fn new(source: Option<S>) -> Self {
        Self(source)
    }
}

impl<S> Observable for OptionObservable<S>
where
    S: Observable,
    S::Item: ToOwned,
{
    type Item = Option<<S::Item as ToOwned>::Owned>;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        if let Some(s) = &self.0 {
            s.with(|value, cx| f(&Some(value.to_owned()), cx), cx)
        } else {
            f(&None, cx)
        }
    }
    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        if let Some(s) = self.0 {
            Obs(s).map(|value| Some(value.to_owned())).into_dyn()
        } else {
            DynObs::new_static(&None)
        }
    }
}

pub struct ResultObservable<S, E>(Result<S, Result<<S::Item as ToOwned>::Owned, E>>)
where
    S: Observable,
    S::Item: ToOwned;

impl<S, E> ResultObservable<S, E>
where
    S: Observable,
    S::Item: ToOwned,
    E: 'static,
{
    pub(crate) fn new(result: Result<S, E>) -> Self {
        Self(match result {
            Ok(s) => Ok(s),
            Err(e) => Err(Err(e)),
        })
    }
}

impl<S, E> Observable for ResultObservable<S, E>
where
    S: Observable,
    S::Item: ToOwned,
    E: 'static,
{
    type Item = Result<<S::Item as ToOwned>::Owned, E>;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        match &self.0 {
            Ok(s) => s.with(|value, cx| f(&Ok(value.to_owned()), cx), cx),
            Err(e) => f(&e, cx),
        }
    }
    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        match self.0 {
            Ok(s) => Obs(s).map(|value| Ok(value.to_owned())).into_dyn(),
            Err(e) => obs_constant(e).into_dyn(),
        }
    }
}
