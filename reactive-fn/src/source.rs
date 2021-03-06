use super::*;
use std::{borrow::Borrow, rc::Rc, sync::Arc};

#[derive(Clone)]
pub enum Source<T>
where
    T: ?Sized + ToOwned + 'static,
{
    Constant(T::Owned),
    Obs(DynObs<T>),
}

impl<T> Source<T>
where
    T: ?Sized + ToOwned + 'static,
{
    pub fn obs(self) -> Obs<impl Observable<Item = T>> {
        Obs(self)
    }
    pub fn into_dyn(self) -> DynObs<T> {
        todo!()
        // match self {
        //     Source::Constant(value) => DynObs::new_constant_map(value, |value| value.borrow()),
        //     Source::Obs(o) => o.into_dyn(),
        // }
    }

    pub fn get(&self, cx: &mut BindContext) -> T::Owned {
        match self {
            Self::Constant(value) => value.borrow().to_owned(),
            Self::Obs(obs) => obs.get(cx),
        }
    }
    pub fn get_head(self) -> T::Owned {
        match self {
            Self::Constant(value) => value.borrow().to_owned(),
            Self::Obs(obs) => obs.get_head(),
        }
    }
    pub fn with<U>(&self, f: impl FnOnce(&T, &mut BindContext) -> U, cx: &mut BindContext) -> U {
        match self {
            Self::Constant(value) => f(value.borrow(), cx),
            Self::Obs(obs) => obs.with(|value, cx| f(value, cx), cx),
        }
    }
    pub fn with_head<U>(&self, f: impl FnOnce(&T) -> U) -> U {
        match self {
            Self::Constant(value) => f(value.borrow()),
            Self::Obs(obs) => obs.with_head(f),
        }
    }

    // pub fn head_tail(self) -> (T, DynTail<T>) {
    //     BindScope::with(|scope| self.head_tail_with(scope))
    // }
    // pub fn head_tail_with(self, scope: &BindScope) -> (T, DynTail<T>) {
    //     match self {
    //         Source::Constant(x) => (x, DynTail::empty()),
    //         Source::Obs(obs) => obs.head_tail_with(scope),
    //     }
    // }
    // pub fn map<U>(self, f: impl Fn(T) -> U + 'static) -> Source<U>
    // where
    //     T: Sized + ToOwned<Owned = T>,
    //     U: ToOwned,
    // {
    //     match self {
    //         Source::Constant(value) => Source::Constant(f(value)),
    //         Source::Obs(o) => Source::Obs(o.map(f)),
    //     }
    // }

    // pub fn fold<St: 'static>(
    //     self,
    //     initial_state: St,
    //     f: impl Fn(St, T) -> St + 'static,
    // ) -> Fold<St> {
    //     match self {
    //         Source::Constant(x) => Fold::constant(f(initial_state, x)),
    //         Source::Obs(obs) => obs.fold(initial_state, f),
    //     }
    // }
    // pub fn collect_to<E: Extend<T> + 'static>(self, e: E) -> Fold<E> {
    //     match self {
    //         Source::Constant(x) => {
    //             let mut e = e;
    //             e.extend(once(x));
    //             Fold::constant(e)
    //         }
    //         Source::Obs(obs) => obs.collect_to(e),
    //     }
    // }
    // pub fn collect<E: Extend<T> + Default + 'static>(self) -> Fold<E> {
    //     self.collect_to(Default::default())
    // }
    // pub fn collect_vec(self) -> Fold<Vec<T>> {
    //     self.collect()
    // }

    // pub fn subscribe(self, f: impl FnMut(T) + 'static) -> Subscription {
    //     match self {
    //         Source::Constant(x) => {
    //             let mut f = f;
    //             f(x);
    //             Subscription::empty()
    //         }
    //         Source::Obs(obs) => obs.subscribe(f),
    //     }
    // }
}
impl<T> Observable for Source<T>
where
    T: ?Sized + ToOwned + 'static,
{
    type Item = T;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        self.with(f, cx)
    }
    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        self.into_dyn()
    }
}

impl<T, S> From<Obs<S>> for Source<T>
where
    S: Observable,
    S::Item: Copy + Into<T>,
    T: ToOwned,
{
    fn from(_value: Obs<S>) -> Self {
        todo!()
        //Source::Obs(value.map_into().into_dyn())
    }
}
impl<T, S> From<&Obs<S>> for Source<T>
where
    S: Observable,
    S::Item: Copy + Into<T>,
    T: ToOwned,
{
    fn from(value: &Obs<S>) -> Self {
        value.clone().into()
    }
}

impl<T, S> From<DynObs<S>> for Source<T>
where
    S: Observable,
    S::Item: Copy + Into<T>,
    T: ToOwned,
{
    fn from(_value: DynObs<S>) -> Self {
        todo!()
        //Source::Obs(value.map_into())
    }
}
impl<T, S> From<&DynObs<S>> for Source<T>
where
    S: Observable,
    S::Item: Copy + Into<T>,
    T: ToOwned,
{
    fn from(value: &DynObs<S>) -> Self {
        value.clone().into()
    }
}

impl<T: Copy> From<&Source<T>> for Source<T> {
    fn from(value: &Source<T>) -> Self {
        value.clone()
    }
}
impl<T> From<Rc<T>> for Source<Rc<T>> {
    fn from(value: Rc<T>) -> Self {
        Source::Constant(value)
    }
}
impl<T> From<&Rc<T>> for Source<Rc<T>> {
    fn from(value: &Rc<T>) -> Self {
        value.clone().into()
    }
}

impl<T> From<Arc<T>> for Source<Arc<T>> {
    fn from(value: Arc<T>) -> Self {
        Source::Constant(value)
    }
}
impl<T> From<&Arc<T>> for Source<Arc<T>> {
    fn from(value: &Arc<T>) -> Self {
        value.clone().into()
    }
}

impl<T> From<ObsCell<T>> for Source<ObsCell<T>> {
    fn from(value: ObsCell<T>) -> Source<ObsCell<T>> {
        Source::Constant(value)
    }
}
impl<T> From<&ObsCell<T>> for Source<ObsCell<T>> {
    fn from(value: &ObsCell<T>) -> Source<ObsCell<T>> {
        value.clone().into()
    }
}

impl<T: Copy> From<ObsCell<T>> for Source<T> {
    fn from(value: ObsCell<T>) -> Self {
        value.obs().into()
    }
}
impl<T: Copy> From<&ObsCell<T>> for Source<T> {
    fn from(value: &ObsCell<T>) -> Self {
        value.clone().into()
    }
}

impl<T> From<Option<T>> for Source<Option<T>>
where
    T: Clone + Into<Source<T>> + 'static,
{
    fn from(value: Option<T>) -> Self {
        Source::Constant(value)
    }
}
impl<T, E> From<Result<T, E>> for Source<Result<T, E>>
where
    T: Clone + Into<Source<T>> + 'static,
    E: Clone + Into<Source<E>> + 'static,
{
    fn from(value: Result<T, E>) -> Source<Result<T, E>> {
        Source::Constant(value)
    }
}

#[macro_export]
macro_rules! impl_from_for_source {
    ($($t:ty),*) => { $(
        impl From<$t> for Source<$t> {
            fn from(value: $t) -> Source<$t> {
                Source::Constant(value)
            }
        }
        impl From<&$t> for Source<$t> {
            fn from(value: &$t) -> Source<$t> {
                value.clone().into()
            }
        }
    )*
    };
}
impl_from_for_source!(u8, u16, u32, u64, u128, usize);
impl_from_for_source!(i8, i16, i32, i64, i128, isize);
impl_from_for_source!(f32, f64);
impl_from_for_source!(bool, char);
