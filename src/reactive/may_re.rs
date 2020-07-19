use super::*;

#[derive(Clone)]
pub struct MayRe<T: 'static>(MayReData<T>);

#[derive(Clone)]
enum MayReData<T: 'static> {
    Value(T),
    Re(Re<T>),
}

impl<T: 'static> MayRe<T> {
    pub fn fold<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, T) -> St + 'static,
    ) -> Fold<St> {
        match self.0 {
            MayReData::Value(x) => Fold::constant(f(initial_state, x)),
            MayReData::Re(re) => re.fold(initial_state, f),
        }
    }
    pub fn collect_to<E: Extend<T> + 'static>(self, e: E) -> Fold<E> {
        match self.0 {
            MayReData::Value(x) => {
                let mut e = e;
                e.extend(once(x));
                Fold::constant(e)
            }
            MayReData::Re(re) => re.collect_to(e),
        }
    }
    pub fn collect<E: Extend<T> + Default + 'static>(self) -> Fold<E> {
        self.collect_to(Default::default())
    }
    pub fn collect_vec(self) -> Fold<Vec<T>> {
        self.collect()
    }

    pub fn for_each(self, f: impl FnMut(T) + 'static) -> Subscription {
        match self.0 {
            MayReData::Value(x) => {
                let mut f = f;
                f(x);
                Subscription::empty()
            }
            MayReData::Re(re) => re.for_each(f),
        }
    }
}

impl<T> Clone for MayReRef<T>
where
    T: Clone + ?Sized + 'static,
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: 'static> From<T> for MayRe<T> {
    fn from(value: T) -> Self {
        MayRe(MayReData::Value(value))
    }
}

impl<T: Copy + 'static> From<&T> for MayRe<T> {
    fn from(value: &T) -> Self {
        MayRe(MayReData::Value(*value))
    }
}
impl<T: 'static> From<Re<T>> for MayRe<T> {
    fn from(source: Re<T>) -> Self {
        MayRe(MayReData::Re(source))
    }
}
impl<T: 'static> From<&Re<T>> for MayRe<T> {
    fn from(source: &Re<T>) -> Self {
        MayRe(MayReData::Re(source.clone()))
    }
}
impl<T: Copy + 'static> From<ReRef<T>> for MayRe<T> {
    fn from(source: ReRef<T>) -> Self {
        MayRe(MayReData::Re(source.cloned()))
    }
}
impl<T: Copy + 'static> From<&ReRef<T>> for MayRe<T> {
    fn from(source: &ReRef<T>) -> Self {
        MayRe(MayReData::Re(source.cloned()))
    }
}
impl<T: Copy + 'static> From<ReBorrow<T>> for MayRe<T> {
    fn from(source: ReBorrow<T>) -> Self {
        MayRe(MayReData::Re(source.cloned()))
    }
}
impl<T: Copy + 'static> From<&ReBorrow<T>> for MayRe<T> {
    fn from(source: &ReBorrow<T>) -> Self {
        MayRe(MayReData::Re(source.cloned()))
    }
}
