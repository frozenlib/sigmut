use super::*;

#[derive(Clone)]
pub enum MayObs<T: 'static> {
    Constant(T),
    Obs(DynObs<T>),
}

impl<T: 'static> MayObs<T> {
    pub fn head_tail(self) -> (T, DynTail<T>) {
        BindScope::with(|scope| self.head_tail_with(scope))
    }
    pub fn head_tail_with(self, scope: &BindScope) -> (T, DynTail<T>) {
        match self {
            MayObs::Constant(x) => (x, DynTail::empty()),
            MayObs::Obs(obs) => obs.head_tail_with(scope),
        }
    }
    pub fn fold<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, T) -> St + 'static,
    ) -> Fold<St> {
        match self {
            MayObs::Constant(x) => Fold::constant(f(initial_state, x)),
            MayObs::Obs(obs) => obs.fold(initial_state, f),
        }
    }
    pub fn collect_to<E: Extend<T> + 'static>(self, e: E) -> Fold<E> {
        match self {
            MayObs::Constant(x) => {
                let mut e = e;
                e.extend(once(x));
                Fold::constant(e)
            }
            MayObs::Obs(obs) => obs.collect_to(e),
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
            MayObs::Constant(x) => {
                let mut f = f;
                f(x);
                Subscription::empty()
            }
            MayObs::Obs(obs) => obs.for_each(f),
        }
    }
}

pub trait IntoMayObs<T> {
    fn into_may_obs(self) -> MayObs<T>;
}

impl<T> IntoMayObs<T> for MayObs<T> {
    fn into_may_obs(self) -> MayObs<T> {
        self
    }
}
impl<T> IntoMayObs<T> for T {
    fn into_may_obs(self) -> MayObs<T> {
        MayObs::Constant(self)
    }
}
impl<T: Copy> IntoMayObs<T> for &T {
    fn into_may_obs(self) -> MayObs<T> {
        MayObs::Constant(*self)
    }
}

impl<T> IntoMayObs<T> for DynObs<T> {
    fn into_may_obs(self) -> MayObs<T> {
        MayObs::Obs(self)
    }
}
impl<T> IntoMayObs<T> for &DynObs<T> {
    fn into_may_obs(self) -> MayObs<T> {
        MayObs::Obs(self.clone())
    }
}
impl<T: Copy + 'static> IntoMayObs<T> for DynObsRef<T> {
    fn into_may_obs(self) -> MayObs<T> {
        self.cloned().into_may_obs()
    }
}
impl<T: Copy + 'static> IntoMayObs<T> for &DynObsRef<T> {
    fn into_may_obs(self) -> MayObs<T> {
        self.cloned().into_may_obs()
    }
}
impl<T: Copy + 'static> IntoMayObs<T> for DynObsBorrow<T> {
    fn into_may_obs(self) -> MayObs<T> {
        self.cloned().into_may_obs()
    }
}
impl<T: Copy + 'static> IntoMayObs<T> for &DynObsBorrow<T> {
    fn into_may_obs(self) -> MayObs<T> {
        self.cloned().into_may_obs()
    }
}
