use futures::{Future, Stream};

use crate::*;
use crate::{hot::*, into_stream::IntoStream, map_async::MapAsync};
use std::{
    any::{Any, TypeId},
    borrow::Borrow,
    iter::once,
    task::Poll,
};

#[derive(Clone)]
pub struct Obs<S>(pub(super) S);

impl<S: Observable> Obs<S> {
    pub fn into_dyn(self) -> DynObs<S::Item> {
        self.0.into_dyn()
    }

    pub fn get_head_tail(self) -> (<S::Item as ToOwned>::Owned, Tail<S>)
    where
        S::Item: ToOwned,
    {
        self.with_head_tail(|value| value.to_owned())
    }
    pub fn with_head_tail<U>(self, f: impl FnOnce(&S::Item) -> U) -> (U, Tail<S>) {
        BindScope::with(|scope| Tail::new(self.0, scope, f))
    }

    #[inline]
    pub fn map<T: 'static>(
        self,
        f: impl Fn(&S::Item) -> T + 'static,
    ) -> Obs<impl Observable<Item = T>> {
        obs(move |cx| self.with(|x, _| f(x), cx))
    }

    #[inline]
    pub fn map_ref<T: ?Sized + 'static>(
        self,
        f: impl Fn(&S::Item) -> &T + 'static,
    ) -> Obs<impl Observable<Item = T>> {
        struct MapRefObservable<S, F> {
            s: S,
            f: F,
        }
        impl<S, F, T> Observable for MapRefObservable<S, F>
        where
            S: Observable,
            F: Fn(&S::Item) -> &T + 'static,
            T: ?Sized + 'static,
        {
            type Item = T;

            #[inline]
            fn with<U>(
                &self,
                f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
                cx: &mut BindContext,
            ) -> U {
                self.s.with(|value, cx| f((self.f)(value), cx), cx)
            }
        }

        Obs(MapRefObservable { s: self, f })
    }

    pub fn map_borrow<B: ?Sized + 'static>(self) -> Obs<impl Observable<Item = B>>
    where
        S::Item: Borrow<B>,
    {
        Obs(ConvertRefObservable::new(self, |x| x.borrow()))
    }

    pub fn map_as_ref<U: ?Sized + 'static>(self) -> Obs<impl Observable<Item = U>>
    where
        S::Item: AsRef<U>,
    {
        Obs(ConvertRefObservable::new(self, |x| x.as_ref()))
    }
    pub fn map_into<U: 'static>(self) -> Obs<impl Observable<Item = U>>
    where
        S::Item: Clone + Into<U>,
    {
        Obs(ConvertValueObservable::new(self, |x| x.clone().into()))
    }

    #[inline]
    pub fn flat_map<U: Observable>(
        self,
        f: impl Fn(&S::Item) -> U + 'static,
    ) -> Obs<impl Observable<Item = U::Item>> {
        self.map(f).flatten()
    }

    #[inline]
    pub fn flat_map_ref<U: Observable>(
        self,
        f: impl Fn(&S::Item) -> &U + 'static,
    ) -> Obs<impl Observable<Item = U::Item>> {
        self.map_ref(f).flatten()
    }

    #[inline]
    pub fn flatten(self) -> Obs<impl Observable<Item = <S::Item as Observable>::Item>>
    where
        S::Item: Observable,
    {
        struct Flatten<S>(S);
        impl<S> Observable for Flatten<S>
        where
            S: Observable,
            S::Item: Observable,
        {
            type Item = <S::Item as Observable>::Item;

            #[inline]
            fn with<U>(
                &self,
                f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
                cx: &mut BindContext,
            ) -> U {
                self.0
                    .with(|s, cx| s.with(|value, cx| f(value, cx), cx), cx)
            }
        }
        Obs(Flatten(self))
    }

    pub fn scan<St: 'static>(
        self,
        initial_state: St,
        f: impl FnMut(&mut St, &S::Item) + 'static,
    ) -> Obs<impl Observable<Item = St>> {
        self.scan_with(initial_state, f, MapId)
    }
    pub fn scan_map<St, T>(
        self,
        initial_state: St,
        f: impl FnMut(&mut St, &S::Item) + 'static,
        m: impl Fn(&St) -> T + 'static,
    ) -> Obs<impl Observable<Item = T>>
    where
        St: 'static,
        T: 'static,
    {
        self.scan_with(initial_state, f, MapValue(m))
    }
    pub fn scan_map_ref<St, T>(
        self,
        initial_state: St,
        f: impl FnMut(&mut St, &S::Item) + 'static,
        m: impl Fn(&St) -> &T + 'static,
    ) -> Obs<impl Observable<Item = T>>
    where
        St: 'static,
        T: ?Sized + 'static,
    {
        self.scan_with(initial_state, f, MapRef(m))
    }
    fn scan_with<St: 'static, M: Map<St>>(
        self,
        initial_state: St,
        mut f: impl FnMut(&mut St, &S::Item) + 'static,
        m: M,
    ) -> Obs<impl Observable<Item = M::Output>> {
        obs_scan_with(
            initial_state,
            move |st, cx| {
                let f = &mut f;
                self.with(|value, _cx| f(st, value), cx)
            },
            m,
        )
    }
    pub fn filter_scan<St: 'static>(
        self,
        initial_state: St,
        predicate: impl Fn(&St, &S::Item) -> bool + 'static,
        f: impl FnMut(&mut St, &S::Item) + 'static,
    ) -> Obs<impl Observable<Item = St>> {
        self.filter_scan_with(initial_state, predicate, f, MapId)
    }
    pub fn filter_scan_map<St: 'static, T: 'static>(
        self,
        initial_state: St,
        predicate: impl Fn(&St, &S::Item) -> bool + 'static,
        f: impl FnMut(&mut St, &S::Item) + 'static,
        m: impl Fn(&St) -> T + 'static,
    ) -> Obs<impl Observable<Item = T>> {
        self.filter_scan_with(initial_state, predicate, f, MapValue(m))
    }
    pub fn filter_scan_map_ref<St: 'static, T: ?Sized + 'static>(
        self,
        initial_state: St,
        predicate: impl Fn(&St, &S::Item) -> bool + 'static,
        f: impl FnMut(&mut St, &S::Item) + 'static,
        m: impl Fn(&St) -> &T + 'static,
    ) -> Obs<impl Observable<Item = T>> {
        self.filter_scan_with(initial_state, predicate, f, MapRef(m))
    }
    fn filter_scan_with<St: 'static, M: Map<St>>(
        self,
        initial_state: St,
        predicate: impl Fn(&St, &S::Item) -> bool + 'static,
        mut f: impl FnMut(&mut St, &S::Item) + 'static,
        m: M,
    ) -> Obs<impl Observable<Item = M::Output>> {
        obs_filter_scan_with(
            initial_state,
            move |st, cx| {
                let f = &mut f;
                self.with(
                    |value, _cx| {
                        if predicate(st, value) {
                            f(st, value);
                            true
                        } else {
                            false
                        }
                    },
                    cx,
                )
            },
            m,
        )
    }

    pub fn cached(self) -> Obs<impl Observable<Item = <S::Item as ToOwned>::Owned>>
    where
        S::Item: ToOwned,
    {
        self.scan_map_ref(
            None,
            |st, value| *st = Some(value.to_owned()),
            |st| st.as_ref().unwrap(),
        )
    }

    pub fn dedup_by(
        self,
        eq: impl Fn(&S::Item, &S::Item) -> bool + 'static,
    ) -> Obs<impl Observable<Item = S::Item>>
    where
        S::Item: ToOwned,
    {
        let initial_state: Option<<S::Item as ToOwned>::Owned> = None;
        self.filter_scan_map_ref(
            initial_state,
            move |st, value| {
                if let Some(old) = st {
                    !eq(old.borrow(), value)
                } else {
                    true
                }
            },
            |st, value| *st = Some(value.to_owned()),
            |st| st.as_ref().unwrap().borrow(),
        )
    }
    pub fn dedup_by_key<K>(
        self,
        to_key: impl Fn(&S::Item) -> K + 'static,
    ) -> Obs<impl Observable<Item = S::Item>>
    where
        K: PartialEq,
        S::Item: ToOwned,
    {
        self.dedup_by(move |old, new| to_key(old) == to_key(new))
    }
    pub fn dedup(self) -> Obs<impl Observable<Item = S::Item>>
    where
        S::Item: ToOwned + PartialEq,
    {
        self.dedup_by(move |old, new| old == new)
    }

    pub fn map_async<Fut: Future + 'static>(
        self,
        f: impl Fn(&S::Item, &mut BindContext) -> Fut + 'static,
    ) -> Obs<impl Observable<Item = Poll<Fut::Output>>> {
        Obs(MapAsync::new(move |cx| self.with(&f, cx)))
    }

    pub fn fold<St: 'static>(
        self,
        st: St,
        mut f: impl FnMut(&mut St, &S::Item) + 'static,
    ) -> Fold<St> {
        Fold::new(st, move |st, cx| self.with(|value, _cx| f(st, value), cx))
    }
    pub fn collect_to<E>(self, e: E) -> Fold<E>
    where
        S::Item: ToOwned,
        E: Extend<<S::Item as ToOwned>::Owned> + 'static,
    {
        self.fold(e, |e, x| e.extend(once(x.to_owned())))
    }
    pub fn collect<E>(self) -> Fold<E>
    where
        S::Item: ToOwned,
        E: Extend<<S::Item as ToOwned>::Owned> + Default + 'static,
    {
        self.collect_to(Default::default())
    }
    pub fn collect_vec(self) -> Fold<Vec<<S::Item as ToOwned>::Owned>>
    where
        S::Item: ToOwned,
    {
        self.collect()
    }
    pub fn subscribe(self, mut f: impl FnMut(&S::Item) + 'static) -> Subscription {
        subscribe(move |cx| self.with(|value, _cx| f(value), cx))
    }
    pub fn subscribe_to<O>(self, o: O) -> impl Subscriber<St = O>
    where
        for<'a> O: Observer<&'a S::Item>,
    {
        subscribe_to(o, move |o, cx| self.with(|value, _cx| o.next(value), cx))
    }
    // pub fn subscribe_async_with<Fut>(
    //     self,
    //     f: impl FnMut(&S::Item) -> Fut + 'static,
    //     sp: impl LocalSpawn,
    // ) -> Subscription
    // where
    //     Fut: Future<Output = ()> + 'static,
    // {
    //     let mut f = f;
    //     Fold::new(FoldBy::new(
    //         (),
    //         fold_by_op(
    //             move |_, cx| ((), self.with(|x, _ctx| sp.spawn_local(f(x)), cx)),
    //             |_| (),
    //             |_| (),
    //         ),
    //     ))
    //     .into()
    // }

    pub fn hot(self) -> Obs<impl Observable<Item = S::Item>> {
        Obs(Hot::new(self))
    }

    pub fn stream(self) -> impl Stream<Item = <S::Item as ToOwned>::Owned>
    where
        S::Item: ToOwned,
    {
        IntoStream::new(self)
    }
    pub fn display(self) -> ObsDisplay<impl ObservableDisplay + 'static>
    where
        S::Item: ObservableDisplay,
    {
        self.into_obs_display()
    }
}
impl<S: Observable> Observable for Obs<S> {
    type Item = S::Item;
    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        self.0.with(f, cx)
    }

    fn into_dyn(self) -> DynObs<Self::Item> {
        self.0.into_dyn()
    }
}

struct ConvertValueObservable<S, F> {
    s: S,
    f: F,
}
impl<S, F, T> ConvertValueObservable<S, F>
where
    S: Observable,
    F: Fn(&S::Item) -> T + 'static,
{
    fn new(s: S, f: F) -> Self {
        Self { s, f }
    }
}

impl<S, F, T> Observable for ConvertValueObservable<S, F>
where
    S: Observable,
    F: Fn(&S::Item) -> T + 'static,
{
    type Item = T;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        self.s.with(|value, cx| f(&(self.f)(value), cx), cx)
    }

    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        if TypeId::of::<S::Item>() == TypeId::of::<T>() {
            Any::downcast_ref::<DynObs<T>>(&self.s.into_dyn())
                .unwrap()
                .clone()
        } else {
            Obs(self.s).map(self.f).into_dyn()
        }
    }
}

struct ConvertRefObservable<S, F> {
    s: S,
    f: F,
}
impl<S, F, T> ConvertRefObservable<S, F>
where
    S: Observable,
    F: Fn(&S::Item) -> &T + 'static,
    T: ?Sized,
{
    fn new(s: S, f: F) -> Self {
        Self { s, f }
    }
}

impl<S, F, T> Observable for ConvertRefObservable<S, F>
where
    S: Observable,
    F: Fn(&S::Item) -> &T + 'static,
    T: ?Sized,
{
    type Item = T;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        self.s.with(|value, cx| f((self.f)(value), cx), cx)
    }

    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        if TypeId::of::<S::Item>() == TypeId::of::<T>() {
            Any::downcast_ref::<DynObs<T>>(&self.s.into_dyn())
                .unwrap()
                .clone()
        } else {
            Obs(self.s).map_ref(self.f).into_dyn()
        }
    }
}
