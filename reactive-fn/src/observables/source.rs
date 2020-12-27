use super::*;

#[derive(Clone)]
pub enum Source<T: 'static> {
    Constant(T),
    Obs(DynObs<T>),
}

impl<T: 'static> Source<T> {
    pub fn head_tail(self) -> (T, DynTail<T>) {
        BindScope::with(|scope| self.head_tail_with(scope))
    }
    pub fn head_tail_with(self, scope: &BindScope) -> (T, DynTail<T>) {
        match self {
            Source::Constant(x) => (x, DynTail::empty()),
            Source::Obs(obs) => obs.head_tail_with(scope),
        }
    }
    pub fn fold<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, T) -> St + 'static,
    ) -> Fold<St> {
        match self {
            Source::Constant(x) => Fold::constant(f(initial_state, x)),
            Source::Obs(obs) => obs.fold(initial_state, f),
        }
    }
    pub fn collect_to<E: Extend<T> + 'static>(self, e: E) -> Fold<E> {
        match self {
            Source::Constant(x) => {
                let mut e = e;
                e.extend(once(x));
                Fold::constant(e)
            }
            Source::Obs(obs) => obs.collect_to(e),
        }
    }
    pub fn collect<E: Extend<T> + Default + 'static>(self) -> Fold<E> {
        self.collect_to(Default::default())
    }
    pub fn collect_vec(self) -> Fold<Vec<T>> {
        self.collect()
    }

    pub fn subscribe(self, f: impl FnMut(T) + 'static) -> Subscription {
        match self {
            Source::Constant(x) => {
                let mut f = f;
                f(x);
                Subscription::empty()
            }
            Source::Obs(obs) => obs.subscribe(f),
        }
    }
}
