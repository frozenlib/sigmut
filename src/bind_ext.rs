/*

impl<B: ReactiveRef> RefBindExt<B> {
    pub fn for_each_by<T: 'static>(
        self,
        attach: impl Fn(&B::Item) -> T + 'static,
        detach: impl Fn(T) + 'static,
    ) -> Unbind {
        self.map(attach).for_each_by(|s| s, detach)
    }
    pub fn for_each_async<Fut: Future<Output = ()> + 'static>(
        self,
        f: impl Fn(&B::Item) -> Fut + 'static,
    ) -> Unbind {
        let sp = get_current_local_spawn();
        self.for_each_by(
            move |value| sp.spawn_local_with_handle(f(value)).unwrap(),
            move |_handle| {},
        )
    }

    pub fn map_ref<U: 'static>(
        self,
        f: impl Fn(&B::Item) -> &U + 'static,
    ) -> RefBindExt<impl ReactiveRef<Item = U>> {
        reactive_ref(self, move |this, ctx| Ref::map(this.borrow(ctx), &f))
    }
    pub fn map_with_ctx<U>(
        self,
        f: impl Fn(&B::Item, &mut BindContext) -> U + 'static,
    ) -> BindExt<impl Reactive<Item = U>> {
        reactive(move |ctx| f(&self.borrow(ctx), ctx))
    }
    pub fn flat_map<O: Reactive>(
        self,
        f: impl Fn(&B::Item) -> O + 'static,
    ) -> BindExt<impl Reactive<Item = O::Item>> {
        self.map(f).flatten()
    }
    pub fn map_async<Fut: Future + 'static>(
        self,
        f: impl Fn(&B::Item) -> Fut + 'static,
    ) -> RefBindExt<impl ReactiveRef<Item = Poll<Fut::Output>>> {
        RefBindExt(MapAsync::new(self.map(f)))
    }

    pub fn cloned(self) -> BindExt<impl Reactive<Item = B::Item>>
    where
        B::Item: Clone,
    {
        self.map(|x| x.clone())
    }
}






pub fn constant<T: 'static>(value: T) -> RefBindExt<impl ReactiveRef<Item = T>> {
    struct Constant<T: 'static>(T);
    impl<T> ReactiveRef for Constant<T> {
        type Item = T;
        fn borrow(&self, _: &mut BindContext) -> Ref<Self::Item> {
            Ref::Native(&self.0)
        }
    }
    RefBindExt(Constant(value))
}

*/
