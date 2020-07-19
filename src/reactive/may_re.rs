use super::*;

#[derive(Clone)]
pub enum MayRe<T: 'static> {
    Value(T),
    Re(Re<T>),
}

impl<T: 'static> MayRe<T> {
    pub fn head_tail(self, scope: &BindContextScope) -> (T, Tail<T>) {
        match self {
            MayRe::Value(x) => (x, Tail::empty()),
            MayRe::Re(re) => re.head_tail(scope),
        }
    }
    pub fn fold<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, T) -> St + 'static,
    ) -> Fold<St> {
        match self {
            MayRe::Value(x) => Fold::constant(f(initial_state, x)),
            MayRe::Re(re) => re.fold(initial_state, f),
        }
    }
    pub fn collect_to<E: Extend<T> + 'static>(self, e: E) -> Fold<E> {
        match self {
            MayRe::Value(x) => {
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
            MayRe::Value(x) => {
                let mut f = f;
                f(x);
                Subscription::empty()
            }
            MayRe::Re(re) => re.for_each(f),
        }
    }
}

impl<T> Clone for MayRe<T>
where
    T: Clone + ?Sized + 'static,
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: 'static> From<T> for MayRe<T> {
    fn from(value: T) -> Self {
        MayRe::Value(value)
    }
}

impl<T: Copy + 'static> From<&T> for MayRe<T> {
    fn from(value: &T) -> Self {
        MayRe::Value(*value)
    }
}
impl<T: 'static> From<Re<T>> for MayRe<T> {
    fn from(source: Re<T>) -> Self {
        MayRe::Re(source)
    }
}
impl<T: 'static> From<&Re<T>> for MayRe<T> {
    fn from(source: &Re<T>) -> Self {
        MayRe::Re(source.clone())
    }
}
impl<T: Copy + 'static> From<ReRef<T>> for MayRe<T> {
    fn from(source: ReRef<T>) -> Self {
        MayRe::Re(source.cloned())
    }
}
impl<T: Copy + 'static> From<&ReRef<T>> for MayRe<T> {
    fn from(source: &ReRef<T>) -> Self {
        MayRe::Re(source.cloned())
    }
}
impl<T: Copy + 'static> From<ReBorrow<T>> for MayRe<T> {
    fn from(source: ReBorrow<T>) -> Self {
        MayRe::Re(source.cloned())
    }
}
impl<T: Copy + 'static> From<&ReBorrow<T>> for MayRe<T> {
    fn from(source: &ReBorrow<T>) -> Self {
        MayRe::Re(source.cloned())
    }
}
