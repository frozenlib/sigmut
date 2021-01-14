use std::sync::Arc;

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

pub trait IntoSource<T: 'static> {
    fn into_source(self) -> Source<T>;
}

impl<T: 'static, S: Observable<Item = T>> IntoSource<T> for Obs<S> {
    fn into_source(self) -> Source<T> {
        self.into_dyn().into_source()
    }
}
impl<T: 'static, S: Observable<Item = T> + Clone> IntoSource<T> for &Obs<S> {
    fn into_source(self) -> Source<T> {
        self.clone().into_source()
    }
}
impl<T: Copy + 'static, S: ObservableBorrow<Item = T>> IntoSource<T> for ObsBorrow<S> {
    fn into_source(self) -> Source<T> {
        self.cloned().into_source()
    }
}
impl<T: Copy + 'static, S: ObservableBorrow<Item = T> + Clone> IntoSource<T> for &ObsBorrow<S> {
    fn into_source(self) -> Source<T> {
        self.clone().into_source()
    }
}
impl<T: Copy + 'static, S: ObservableRef<Item = T>> IntoSource<T> for ObsRef<S> {
    fn into_source(self) -> Source<T> {
        self.cloned().into_source()
    }
}
impl<T: Copy + 'static, S: ObservableRef<Item = T> + Clone> IntoSource<T> for &ObsRef<S> {
    fn into_source(self) -> Source<T> {
        self.clone().into_source()
    }
}

impl<T: 'static> IntoSource<T> for DynObs<T> {
    fn into_source(self) -> Source<T> {
        Source::Obs(self)
    }
}
impl<T: 'static> IntoSource<T> for &DynObs<T> {
    fn into_source(self) -> Source<T> {
        self.clone().into_source()
    }
}
impl<T: Copy + 'static> IntoSource<T> for DynObsBorrow<T> {
    fn into_source(self) -> Source<T> {
        Source::Obs(self.cloned())
    }
}
impl<T: Copy + 'static> IntoSource<T> for &DynObsBorrow<T> {
    fn into_source(self) -> Source<T> {
        Source::Obs(self.cloned())
    }
}
impl<T: Copy + 'static> IntoSource<T> for DynObsRef<T> {
    fn into_source(self) -> Source<T> {
        Source::Obs(self.cloned())
    }
}
impl<T: Copy + 'static> IntoSource<T> for &DynObsRef<T> {
    fn into_source(self) -> Source<T> {
        Source::Obs(self.cloned())
    }
}
impl<T> IntoSource<T> for Source<T> {
    fn into_source(self) -> Source<T> {
        self
    }
}
impl<T: 'static> IntoSource<Rc<T>> for Rc<T> {
    fn into_source(self) -> Source<Rc<T>> {
        Source::Constant(self)
    }
}
impl<T: 'static> IntoSource<Arc<T>> for Arc<T> {
    fn into_source(self) -> Source<Arc<T>> {
        Source::Constant(self)
    }
}
impl<T: IntoSource<T> + 'static> IntoSource<Option<T>> for Option<T> {
    fn into_source(self) -> Source<Option<T>> {
        Source::Constant(self)
    }
}
impl<T: IntoSource<T> + 'static, E: IntoSource<E> + 'static> IntoSource<Result<T, E>>
    for Result<T, E>
{
    fn into_source(self) -> Source<Result<T, E>> {
        Source::Constant(self)
    }
}

macro_rules! impl_into_source {
    ($($t:ty),*) => { $(
        impl IntoSource<$t> for $t {
            fn into_source(self) -> Source<$t> {
                Source::Constant(self)
            }
        }
    )*
    };
}

impl_into_source!(u8, u16, u32, u64, u128, usize);
impl_into_source!(i8, i16, i32, i64, i128, isize);
impl_into_source!(f32, f64);
impl_into_source!(bool, char);
