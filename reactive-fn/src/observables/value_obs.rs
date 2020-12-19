use super::*;

#[derive(Clone)]
pub enum ValueObs<T: 'static> {
    Constant(T),
    Obs(DynObs<T>),
}

impl<T: 'static> ValueObs<T> {
    pub fn head_tail(self) -> (T, DynTail<T>) {
        BindScope::with(|scope| self.head_tail_with(scope))
    }
    pub fn head_tail_with(self, scope: &BindScope) -> (T, DynTail<T>) {
        match self {
            ValueObs::Constant(x) => (x, DynTail::empty()),
            ValueObs::Obs(obs) => obs.head_tail_with(scope),
        }
    }
    pub fn fold<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, T) -> St + 'static,
    ) -> Fold<St> {
        match self {
            ValueObs::Constant(x) => Fold::constant(f(initial_state, x)),
            ValueObs::Obs(obs) => obs.fold(initial_state, f),
        }
    }
    pub fn collect_to<E: Extend<T> + 'static>(self, e: E) -> Fold<E> {
        match self {
            ValueObs::Constant(x) => {
                let mut e = e;
                e.extend(once(x));
                Fold::constant(e)
            }
            ValueObs::Obs(obs) => obs.collect_to(e),
        }
    }
    pub fn collect<E: Extend<T> + Default + 'static>(self) -> Fold<E> {
        self.collect_to(Default::default())
    }
    pub fn collect_vec(self) -> Fold<Vec<T>> {
        self.collect()
    }

    pub fn for_each(self, f: impl FnMut(T) + 'static) -> Subscription {
        match self {
            ValueObs::Constant(x) => {
                let mut f = f;
                f(x);
                Subscription::empty()
            }
            ValueObs::Obs(obs) => obs.for_each(f),
        }
    }
}

pub trait IntoValueObs<T> {
    fn into_may_obs(self) -> ValueObs<T>;
}

impl<T> IntoValueObs<T> for ValueObs<T> {
    fn into_may_obs(self) -> ValueObs<T> {
        self
    }
}
impl<T> IntoValueObs<T> for T {
    fn into_may_obs(self) -> ValueObs<T> {
        ValueObs::Constant(self)
    }
}
impl<T: Copy> IntoValueObs<T> for &T {
    fn into_may_obs(self) -> ValueObs<T> {
        ValueObs::Constant(*self)
    }
}

impl<T> IntoValueObs<T> for DynObs<T> {
    fn into_may_obs(self) -> ValueObs<T> {
        ValueObs::Obs(self)
    }
}
impl<T> IntoValueObs<T> for &DynObs<T> {
    fn into_may_obs(self) -> ValueObs<T> {
        ValueObs::Obs(self.clone())
    }
}
impl<T: Copy + 'static> IntoValueObs<T> for DynObsRef<T> {
    fn into_may_obs(self) -> ValueObs<T> {
        self.cloned().into_may_obs()
    }
}
impl<T: Copy + 'static> IntoValueObs<T> for &DynObsRef<T> {
    fn into_may_obs(self) -> ValueObs<T> {
        self.cloned().into_may_obs()
    }
}
impl<T: Copy + 'static> IntoValueObs<T> for DynObsBorrow<T> {
    fn into_may_obs(self) -> ValueObs<T> {
        self.cloned().into_may_obs()
    }
}
impl<T: Copy + 'static> IntoValueObs<T> for &DynObsBorrow<T> {
    fn into_may_obs(self) -> ValueObs<T> {
        self.cloned().into_may_obs()
    }
}
