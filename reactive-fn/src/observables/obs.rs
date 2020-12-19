use super::*;

pub fn obs<T>(get: impl Fn(&BindContext) -> T + 'static) -> Obs<impl Observable<Item = T>> {
    struct ObsFn<F>(F);
    impl<F: Fn(&BindContext) -> T + 'static, T> Observable for ObsFn<F> {
        type Item = T;
        fn get(&self, cx: &BindContext) -> Self::Item {
            (self.0)(cx)
        }
    }

    Obs(ObsFn(get))
}
pub fn obs_constant<T: 'static + Clone>(value: T) -> Obs<impl Observable<Item = T>> {
    obs(move |_| value.clone())
}

#[derive(Clone)]
pub struct Obs<S>(pub(super) S);

impl<S: Observable> Obs<S> {
    pub fn get(&self, cx: &BindContext) -> S::Item {
        self.0.get(cx)
    }
    pub fn with<T>(&self, f: impl FnOnce(&S::Item, &BindContext) -> T, cx: &BindContext) -> T {
        f(&self.get(cx), cx)
    }
    pub fn head_tail(self) -> (S::Item, TailOps<S>) {
        BindScope::with(|scope| self.head_tail_with(scope))
    }
    pub fn head_tail_with(self, scope: &BindScope) -> (S::Item, TailOps<S>) {
        TailOps::new(self.0, scope)
    }

    pub fn as_ref(self) -> ObsRef<ObsRefByObs<S>> {
        ObsRef(ObsRefByObs(self))
    }
    pub fn as_any(self) -> Obs<DynObs<S::Item>> {
        Obs(self.into_dyn())
    }
    pub fn into_dyn(self) -> DynObs<S::Item> {
        self.0.into_dyn()
    }
    pub fn into_dyn_ref(self) -> DynObsRef<S::Item> {
        self.0.into_dyn().as_ref()
    }

    pub fn map<T>(self, f: impl Fn(S::Item) -> T + 'static) -> Obs<impl Observable<Item = T>> {
        obs(move |cx| f(self.get(cx)))
    }
    pub fn flat_map<T: Observable>(
        self,
        f: impl Fn(S::Item) -> T + 'static,
    ) -> Obs<impl Observable<Item = T::Item>> {
        obs(move |cx| f(self.get(cx)).get(cx))
    }
    pub fn flatten(self) -> Obs<impl Observable<Item = <S::Item as Observable>::Item>>
    where
        S::Item: Observable,
    {
        obs(move |cx| self.get(cx).get(cx))
    }
    pub fn map_async_with<Fut>(
        self,
        f: impl Fn(S::Item) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> ObsBorrow<impl ObservableBorrow<Item = Poll<Fut::Output>> + Clone>
    where
        Fut: Future + 'static,
    {
        ObsBorrow(Rc::new(MapAsync::new(self.map(f), sp)))
    }

    pub fn cached(self) -> ObsBorrow<impl ObservableBorrow<Item = S::Item> + Clone> {
        ObsBorrow(Rc::new(Scan::new(
            (),
            scan_op(move |_, cx| self.get(cx), |_| (), |x| x),
        )))
    }
    pub fn scan<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, S::Item) -> St + 'static,
    ) -> ObsBorrow<impl ObservableBorrow<Item = St> + Clone> {
        ObsBorrow(Rc::new(Scan::new(
            initial_state,
            scan_op(move |st, cx| f(st, self.get(cx)), |st| st, |st| st),
        )))
    }
    pub fn filter_scan<St: 'static>(
        self,
        initial_state: St,
        predicate: impl Fn(&St, &S::Item) -> bool + 'static,
        f: impl Fn(St, S::Item) -> St + 'static,
    ) -> ObsBorrow<impl ObservableBorrow<Item = St> + Clone> {
        ObsBorrow(Rc::new(FilterScan::new(
            initial_state,
            filter_scan_op(
                move |state, cx| {
                    let value = self.get(cx);
                    let is_notify = predicate(&state, &value);
                    let state = if is_notify { f(state, value) } else { state };
                    FilterScanLoad { is_notify, state }
                },
                |state| state,
                |state| state,
            ),
        )))
    }
    pub fn dedup_by(
        self,
        eq: impl Fn(&S::Item, &S::Item) -> bool + 'static,
    ) -> ObsBorrow<impl ObservableBorrow<Item = S::Item> + Clone> {
        ObsBorrow(Rc::new(FilterScan::new(
            None,
            filter_scan_op(
                move |state, cx| {
                    let mut value = self.get(cx);
                    let mut is_notify = false;
                    if let Some(old) = state {
                        if eq(&value, &old) {
                            value = old;
                        } else {
                            is_notify = true;
                        }
                    }
                    FilterScanLoad {
                        state: value,
                        is_notify,
                    }
                },
                |value| Some(value),
                |value| value,
            ),
        )))
    }
    pub fn dedup_by_key<K: PartialEq>(
        self,
        to_key: impl Fn(&S::Item) -> K + 'static,
    ) -> ObsBorrow<impl ObservableBorrow<Item = S::Item> + Clone> {
        self.dedup_by(move |l, r| to_key(l) == to_key(r))
    }
    pub fn dedup(self) -> ObsBorrow<impl ObservableBorrow<Item = S::Item> + Clone>
    where
        S::Item: PartialEq,
    {
        self.dedup_by(|l, r| l == r)
    }

    pub fn fold<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, S::Item) -> St + 'static,
    ) -> Fold<St> {
        Fold::new(FoldBy::new(
            initial_state,
            fold_by_op(move |st, cx| f(st, self.get(cx)), |st| st, |st| st),
        ))
    }
    pub fn collect_to<E: Extend<S::Item> + 'static>(self, e: E) -> Fold<E> {
        self.fold(e, |mut e, x| {
            e.extend(once(x));
            e
        })
    }
    pub fn collect<E: Extend<S::Item> + Default + 'static>(self) -> Fold<E> {
        self.collect_to(Default::default())
    }
    pub fn collect_vec(self) -> Fold<Vec<S::Item>> {
        self.collect()
    }
    pub fn for_each(self, f: impl FnMut(S::Item) + 'static) -> Subscription {
        self.fold(f, move |mut f, x| {
            f(x);
            f
        })
        .into()
    }
    pub fn for_each_async_with<Fut>(
        self,
        f: impl FnMut(S::Item) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        let mut f = f;
        Fold::new(FoldBy::new(
            (),
            fold_by_op(
                move |_, cx| ((), sp.spawn_local(f(self.get(cx)))),
                |_| (),
                |_| (),
            ),
        ))
        .into()
    }
    pub fn hot(self) -> Obs<impl Observable<Item = S::Item>> {
        Obs(Hot::new(self))
    }

    pub fn stream(self) -> impl futures::Stream<Item = S::Item> {
        IntoStream::new(self)
    }
}
impl<S: Observable> Observable for Obs<S> {
    type Item = S::Item;
    fn get(&self, cx: &BindContext) -> Self::Item {
        self.0.get(cx)
    }
    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        self.0.into_dyn()
    }
}

#[derive(Clone)]
pub struct ObsRefByObs<S>(Obs<S>);
impl<S: Observable> ObservableRef for ObsRefByObs<S> {
    type Item = S::Item;
    fn with<U>(&self, f: impl FnOnce(&Self::Item, &BindContext) -> U, cx: &BindContext) -> U {
        self.0.with(f, cx)
    }
    fn into_dyn(self) -> DynObsRef<Self::Item>
    where
        Self: Sized,
    {
        self.0.into_dyn_ref()
    }
}
