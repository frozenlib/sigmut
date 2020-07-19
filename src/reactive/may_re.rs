use super::*;

#[derive(Clone)]
pub enum MayRe<T: 'static> {
    Constant(T),
    Re(Re<T>),
}

impl<T: 'static> MayRe<T> {
    pub fn head_tail(self, scope: &BindContextScope) -> (T, Tail<T>) {
        match self {
            MayRe::Constant(x) => (x, Tail::empty()),
            MayRe::Re(re) => re.head_tail(scope),
        }
    }
    pub fn fold<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, T) -> St + 'static,
    ) -> Fold<St> {
        match self {
            MayRe::Constant(x) => Fold::constant(f(initial_state, x)),
            MayRe::Re(re) => re.fold(initial_state, f),
        }
    }
    pub fn collect_to<E: Extend<T> + 'static>(self, e: E) -> Fold<E> {
        match self {
            MayRe::Constant(x) => {
                let mut e = e;
                e.extend(once(x));
                Fold::constant(e)
            }
            MayRe::Re(re) => re.collect_to(e),
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
            MayRe::Constant(x) => {
                let mut f = f;
                f(x);
                Subscription::empty()
            }
            MayRe::Re(re) => re.for_each(f),
        }
    }
}

pub trait IntoMayRe<T> {
    fn into_may_re(self) -> MayRe<T>;
}

impl<T> IntoMayRe<T> for MayRe<T> {
    fn into_may_re(self) -> MayRe<T> {
        self
    }
}
impl<T> IntoMayRe<T> for T {
    fn into_may_re(self) -> MayRe<T> {
        MayRe::Constant(self)
    }
}
impl<T: Copy> IntoMayRe<T> for &T {
    fn into_may_re(self) -> MayRe<T> {
        MayRe::Constant(*self)
    }
}

impl<T> IntoMayRe<T> for Re<T> {
    fn into_may_re(self) -> MayRe<T> {
        MayRe::Re(self)
    }
}
impl<T> IntoMayRe<T> for &Re<T> {
    fn into_may_re(self) -> MayRe<T> {
        MayRe::Re(self.clone())
    }
}
impl<T: Copy + 'static> IntoMayRe<T> for ReRef<T> {
    fn into_may_re(self) -> MayRe<T> {
        self.cloned().into_may_re()
    }
}
impl<T: Copy + 'static> IntoMayRe<T> for &ReRef<T> {
    fn into_may_re(self) -> MayRe<T> {
        self.cloned().into_may_re()
    }
}
impl<T: Copy + 'static> IntoMayRe<T> for ReBorrow<T> {
    fn into_may_re(self) -> MayRe<T> {
        self.cloned().into_may_re()
    }
}
impl<T: Copy + 'static> IntoMayRe<T> for &ReBorrow<T> {
    fn into_may_re(self) -> MayRe<T> {
        self.cloned().into_may_re()
    }
}
