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
        into_dyn_by_convert(self, |s| s.0)
    }
}

pub struct MapBorrowObservable<S: Observable, T: ?Sized>(
    pub(crate) S,
    pub(crate) PhantomData<fn(&S::Item) -> &T>,
);

impl<S, T> Observable for MapBorrowObservable<S, T>
where
    S: Observable,
    S::Item: Borrow<T>,
    T: ?Sized + 'static,
{
    type Item = T;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        self.0.with(|value, cx| f(&value.borrow(), cx), cx)
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
    S: Observable,
    S::Item: AsRef<T>,
    T: ?Sized + 'static,
{
    type Item = T;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        self.0.with(|value, cx| f(&value.as_ref(), cx), cx)
    }
    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        into_dyn_by_convert(self, |s| s.0)
    }
}

fn into_dyn_by_convert<Outer: Observable, Inner: Observable>(
    s: Outer,
    f: impl FnOnce(Outer) -> Inner,
) -> DynObs<Outer::Item> {
    if TypeId::of::<Outer::Item>() == TypeId::of::<Inner>() {
        <dyn Any>::downcast_ref::<DynObs<Outer::Item>>(&f(s).into_dyn())
            .unwrap()
            .clone()
    } else {
        DynObs::new_dyn(Rc::new(DynamicObs(s)))
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

pub struct StaticObservable<T: ?Sized + 'static>(pub(crate) &'static T);

impl<T: ?Sized> Observable for StaticObservable<T> {
    type Item = T;

    #[inline]
    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        f(self.0, cx)
    }
    #[inline]
    fn into_dyn(self) -> DynObs<Self::Item> {
        DynObs::new_static(self.0)
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
