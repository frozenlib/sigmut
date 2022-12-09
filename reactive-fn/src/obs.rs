use crate::*;
use crate::{
    hot::*, into_stream::IntoStream, map_async::MapAsync, map_stream::MapStream, observables::*,
};
use futures_core::{Future, Stream};
use std::marker::PhantomData;
use std::{borrow::Borrow, iter::once, task::Poll};

#[derive(Clone)]
pub struct ImplObs<S>(pub(crate) S);

impl<S: Observable + 'static> ImplObs<S> {
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
    ) -> ImplObs<impl Observable<Item = T>> {
        obs(move |bc| self.with(|x, _| f(x), bc))
    }

    #[inline]
    pub fn map_ref<T: ?Sized + 'static>(
        self,
        f: impl Fn(&S::Item) -> &T + 'static,
    ) -> ImplObs<impl Observable<Item = T>> {
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
                bc: &mut BindContext,
            ) -> U {
                self.s.with(|value, bc| f((self.f)(value), bc), bc)
            }
        }

        ImplObs(MapRefObservable { s: self, f })
    }

    pub fn map_borrow<B: ?Sized + 'static>(self) -> ImplObs<MapBorrowObservable<S, B>>
    where
        S::Item: Borrow<B>,
    {
        ImplObs(MapBorrowObservable(self.0, PhantomData))
    }

    pub fn map_as_ref<U: ?Sized + 'static>(self) -> ImplObs<MapAsRefObservable<S, U>>
    where
        S::Item: AsRef<U>,
    {
        ImplObs(MapAsRefObservable(self.0, PhantomData))
    }
    pub fn map_into<U: 'static>(self) -> ImplObs<MapIntoObservable<S, U>>
    where
        S::Item: Clone + Into<U>,
    {
        ImplObs(MapIntoObservable(self.0, PhantomData))
    }

    #[inline]
    pub fn flat_map<U: Observable + 'static>(
        self,
        f: impl Fn(&S::Item) -> U + 'static,
    ) -> ImplObs<impl Observable<Item = U::Item>> {
        self.map(f).flatten()
    }

    #[inline]
    pub fn flat_map_ref<U: Observable + 'static>(
        self,
        f: impl Fn(&S::Item) -> &U + 'static,
    ) -> ImplObs<impl Observable<Item = U::Item>> {
        self.map_ref(f).flatten()
    }

    #[inline]
    pub fn flatten(self) -> ImplObs<impl Observable<Item = <S::Item as Observable>::Item>>
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
                bc: &mut BindContext,
            ) -> U {
                self.0
                    .with(|s, bc| s.with(|value, bc| f(value, bc), bc), bc)
            }
        }
        ImplObs(Flatten(self))
    }

    pub fn scan<St: 'static>(
        self,
        initial_state: St,
        f: impl FnMut(&mut St, &S::Item) + 'static,
    ) -> ImplObs<impl Observable<Item = St>> {
        self.scan_with(initial_state, f, MapId)
    }
    pub fn scan_map<St, T>(
        self,
        initial_state: St,
        f: impl FnMut(&mut St, &S::Item) + 'static,
        m: impl Fn(&St) -> T + 'static,
    ) -> ImplObs<impl Observable<Item = T>>
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
    ) -> ImplObs<impl Observable<Item = T>>
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
    ) -> ImplObs<impl Observable<Item = M::Output>> {
        obs_scan_with(
            initial_state,
            move |st, bc| {
                let f = &mut f;
                self.with(|value, _bc| f(st, value), bc)
            },
            m,
        )
    }
    pub fn filter_scan<St: 'static>(
        self,
        initial_state: St,
        predicate: impl Fn(&St, &S::Item) -> bool + 'static,
        f: impl FnMut(&mut St, &S::Item) + 'static,
    ) -> ImplObs<impl Observable<Item = St>> {
        self.filter_scan_with(initial_state, predicate, f, MapId)
    }
    pub fn filter_scan_map<St: 'static, T: 'static>(
        self,
        initial_state: St,
        predicate: impl Fn(&St, &S::Item) -> bool + 'static,
        f: impl FnMut(&mut St, &S::Item) + 'static,
        m: impl Fn(&St) -> T + 'static,
    ) -> ImplObs<impl Observable<Item = T>> {
        self.filter_scan_with(initial_state, predicate, f, MapValue(m))
    }
    pub fn filter_scan_map_ref<St: 'static, T: ?Sized + 'static>(
        self,
        initial_state: St,
        predicate: impl Fn(&St, &S::Item) -> bool + 'static,
        f: impl FnMut(&mut St, &S::Item) + 'static,
        m: impl Fn(&St) -> &T + 'static,
    ) -> ImplObs<impl Observable<Item = T>> {
        self.filter_scan_with(initial_state, predicate, f, MapRef(m))
    }
    fn filter_scan_with<St: 'static, M: Map<St>>(
        self,
        initial_state: St,
        predicate: impl Fn(&St, &S::Item) -> bool + 'static,
        mut f: impl FnMut(&mut St, &S::Item) + 'static,
        m: M,
    ) -> ImplObs<impl Observable<Item = M::Output>> {
        obs_filter_scan_with(
            initial_state,
            move |st, bc| {
                let f = &mut f;
                self.with(
                    |value, _bc| {
                        if predicate(st, value) {
                            f(st, value);
                            true
                        } else {
                            false
                        }
                    },
                    bc,
                )
            },
            m,
        )
    }

    pub fn cached(self) -> ImplObs<impl Observable<Item = <S::Item as ToOwned>::Owned>>
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
    ) -> ImplObs<impl Observable<Item = S::Item>>
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
    ) -> ImplObs<impl Observable<Item = S::Item>>
    where
        K: PartialEq,
        S::Item: ToOwned,
    {
        self.dedup_by(move |old, new| to_key(old) == to_key(new))
    }
    pub fn dedup(self) -> ImplObs<impl Observable<Item = S::Item>>
    where
        S::Item: ToOwned + PartialEq,
    {
        self.dedup_by(move |old, new| old == new)
    }

    pub fn map_async<Fut: Future + 'static>(
        self,
        f: impl Fn(&S::Item) -> Fut + 'static,
    ) -> ImplObs<impl Observable<Item = Poll<Fut::Output>>> {
        ImplObs(MapAsync::new(move |bc| {
            self.with(|value, _bc| f(value), bc)
        }))
    }
    pub fn map_stream<St: Stream + 'static>(
        self,
        initial_value: St::Item,
        f: impl Fn(&S::Item) -> St + 'static,
    ) -> ImplObs<impl Observable<Item = St::Item>> {
        ImplObs(MapStream::new(initial_value, move |bc| {
            self.with(|value, _bc| f(value), bc)
        }))
    }

    pub fn fold<St: 'static>(
        self,
        st: St,
        mut f: impl FnMut(&mut St, &S::Item) + 'static,
    ) -> Fold<St> {
        Fold::new(st, move |st, bc| self.with(|value, _bc| f(st, value), bc))
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
        subscribe(move |bc| self.with(|value, _bc| f(value), bc))
    }
    pub fn subscribe_to<O>(self, o: O) -> impl Subscriber<St = O>
    where
        for<'a> O: Observer<&'a S::Item>,
    {
        subscribe_to(o, move |o, bc| self.with(|value, _bc| o.next(value), bc))
    }
    pub fn subscribe_async<Fut>(self, mut f: impl FnMut(&S::Item) -> Fut + 'static) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        subscribe_async(move |bc| self.with(|value, _bc| f(value), bc))
    }

    pub fn hot(self) -> ImplObs<impl Observable<Item = S::Item>> {
        ImplObs(Hot::new(self))
    }

    pub fn stream(self) -> impl Stream<Item = <S::Item as ToOwned>::Owned>
    where
        S::Item: ToOwned,
    {
        IntoStream::new(self)
    }
    pub fn may(self) -> MayObs<S::Item>
    where
        S::Item: Sized,
    {
        self.0.into_may()
    }
    pub fn display(self) -> ObsDisplay<impl ObservableDisplay + 'static>
    where
        S::Item: ObservableDisplay,
    {
        self.into_obs_display()
    }
}
impl<S: Observable + 'static> Observable for ImplObs<S> {
    type Item = S::Item;
    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        bc: &mut BindContext,
    ) -> U {
        self.0.with(f, bc)
    }

    fn into_dyn(self) -> DynObs<Self::Item> {
        self.0.into_dyn()
    }
}
