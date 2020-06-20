use super::*;

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub struct ReRef<T: 'static + ?Sized>(pub(super) ReRefData<T>);

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub(super) enum ReRefData<T: 'static + ?Sized> {
    StaticRef(&'static T),
    Dyn(Rc<dyn DynamicReactiveRef<Item = T>>),
    DynSource(Rc<dyn DynamicReactiveRefSource<Item = T>>),
}

impl<T: 'static + ?Sized> ReRef<T> {
    pub fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &T) -> U) -> U {
        if let ReRefData::StaticRef(x) = &self.0 {
            f(ctx, x)
        } else {
            let mut output = None;
            let mut f = Some(f);
            self.dyn_with(ctx, &mut |ctx, value| {
                output = Some((f.take().unwrap())(ctx, value))
            });
            output.unwrap()
        }
    }
    fn dyn_with(&self, ctx: &BindContext, f: &mut dyn FnMut(&BindContext, &T)) {
        match &self.0 {
            ReRefData::StaticRef(x) => f(ctx, x),
            ReRefData::Dyn(rc) => rc.dyn_with(ctx, f),
            ReRefData::DynSource(rc) => rc.clone().dyn_with(ctx, f),
        }
    }

    pub fn head_tail(&self, scope: &BindContextScope, f: impl FnOnce(&T)) -> TailRef<T> {
        TailRef::new(self.clone(), scope, f)
    }
    pub fn new<S: 'static>(
        this: S,
        f: impl Fn(&S, &BindContext, &mut dyn FnMut(&BindContext, &T)) + 'static,
    ) -> Self {
        struct ReRefFn<S, T: ?Sized, F> {
            this: S,
            f: F,
            _phantom: PhantomData<fn(&Self) -> &T>,
        }
        impl<S, T, F> DynamicReactiveRef for ReRefFn<S, T, F>
        where
            S: 'static,
            T: 'static + ?Sized,
            F: Fn(&S, &BindContext, &mut dyn FnMut(&BindContext, &T)) + 'static,
        {
            type Item = T;

            fn dyn_with(&self, ctx: &BindContext, f: &mut dyn FnMut(&BindContext, &T)) {
                (self.f)(&self.this, ctx, f)
            }
        }
        Self::from_dyn(ReRefFn {
            this,
            f,
            _phantom: PhantomData,
        })
    }
    pub fn constant(value: T) -> Self
    where
        T: Sized,
    {
        Self::new(value, |value, ctx, f| f(ctx, value))
    }
    pub fn static_ref(value: &'static T) -> Self {
        Self(ReRefData::StaticRef(value))
    }

    pub fn ops(&self) -> ReRefOps<Self> {
        ReRefOps(self.clone())
    }

    pub(super) fn from_dyn(inner: impl DynamicReactiveRef<Item = T>) -> Self {
        Self(ReRefData::Dyn(Rc::new(inner)))
    }

    pub fn map<U>(&self, f: impl Fn(&T) -> U + 'static) -> Re<U> {
        let this = self.clone();
        Re::new(move |ctx| this.with(ctx, |_ctx, x| f(x)))
    }
    pub fn map_ref<U: ?Sized>(&self, f: impl Fn(&T) -> &U + 'static) -> ReRef<U> {
        if let ReRefData::StaticRef(x) = &self.0 {
            ReRef::static_ref(f(x))
        } else {
            ReRef::new(self.clone(), move |this, ctx, f_inner| {
                this.with(ctx, |ctx, x| f_inner(ctx, f(x)))
            })
        }
    }
    pub fn map_borrow<B: ?Sized>(&self) -> ReRef<B>
    where
        T: Borrow<B>,
    {
        if let Some(b) = Any::downcast_ref::<ReRef<B>>(self) {
            b.clone()
        } else {
            self.map_ref(|x| x.borrow())
        }
    }

    pub fn flat_map<U>(&self, f: impl Fn(&T) -> Re<U> + 'static) -> Re<U> {
        self.map(f).flatten()
    }
    pub fn map_async_with<Fut>(
        &self,
        f: impl Fn(&T) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> ReBorrow<Poll<Fut::Output>>
    where
        Fut: Future + 'static,
    {
        ReBorrow::from_dyn_source(MapAsync::new(self.map(f), sp))
    }
    pub fn scan<St: 'static>(
        &self,
        initial_state: St,
        f: impl Fn(St, &T) -> St + 'static,
    ) -> ReBorrow<St> {
        let this = self.clone();
        ReBorrow::from_dyn_source(Scan::new(
            initial_state,
            move |st, ctx| {
                let f = &f;
                this.with(ctx, move |_ctx, x| f(st, x))
            },
            |st| st,
            |st| st,
        ))
    }
    pub fn filter_scan<St: 'static>(
        &self,
        initial_state: St,
        predicate: impl Fn(&St, &T) -> bool + 'static,
        f: impl Fn(St, &T) -> St + 'static,
    ) -> ReBorrow<St> {
        let this = self.clone();
        ReBorrow::from_dyn_source(FilterScan::new(
            initial_state,
            move |state, ctx| {
                this.with(ctx, |_ctx, value| {
                    let is_notify = predicate(&state, &value);
                    let state = if is_notify { f(state, value) } else { state };
                    FilterScanResult { is_notify, state }
                })
            },
            |state| state,
            |state| state,
        ))
    }

    pub fn cloned(&self) -> Re<T>
    where
        T: Clone,
    {
        self.map(|x| x.clone())
    }
    pub fn fold<St: 'static>(
        &self,
        initial_state: St,
        f: impl Fn(St, &T) -> St + 'static,
    ) -> Fold<St> {
        let this = self.clone();
        let mut f = f;
        Fold::new(FoldBy::new(
            initial_state,
            move |st, ctx| {
                let f = &mut f;
                (this.with(ctx, move |_ctx, x| f(st, x)), ())
            },
            |(st, _)| st,
            |st| st,
        ))
    }
    pub fn collect_to<E: for<'a> Extend<&'a T> + 'static>(&self, e: E) -> Fold<E> {
        self.fold(e, |mut e, x| {
            e.extend(once(x));
            e
        })
    }
    pub fn collect<E: for<'a> Extend<&'a T> + Default + 'static>(&self) -> Fold<E> {
        self.collect_to(Default::default())
    }
    pub fn collect_vec(&self) -> Fold<Vec<T>>
    where
        T: Copy,
    {
        self.collect()
    }
    pub fn for_each(&self, f: impl FnMut(&T) + 'static) -> Subscription {
        self.fold(f, move |mut f, x| {
            f(x);
            f
        })
        .into()
    }
    pub fn for_each_async_with<Fut>(
        &self,
        f: impl FnMut(&T) -> Fut + 'static,
        sp: impl LocalSpawn,
    ) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        let this = self.clone();
        let mut f = f;
        Fold::new(FoldBy::new(
            (),
            move |_, ctx| ((), this.with(ctx, |_ctx, x| sp.spawn_local(f(x)))),
            |_| (),
            |_| (),
        ))
        .into()
    }

    pub fn hot(&self) -> Self {
        Self(ReRefData::Dyn(Hot::new(self.ops())))
    }
}
impl<T: ?Sized> ReactiveRef for ReRef<T> {
    type Item = T;

    fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &Self::Item) -> U) -> U {
        ReRef::with(self, ctx, f)
    }
    fn into_dyn(self) -> ReRef<Self::Item>
    where
        Self: Sized,
    {
        self
    }
}
