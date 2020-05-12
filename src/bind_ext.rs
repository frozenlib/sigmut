/*

impl<B: Reactive> BindExt<B> {
    pub fn for_each(self, f: impl Fn(B::Item) + 'static) -> Unbind {
        Unbind(ForEach::new(self, f))
    }
    pub fn for_each_by<T: 'static>(
        self,
        attach: impl Fn(B::Item) -> T + 'static,
        detach: impl Fn(T) + 'static,
    ) -> Unbind {
        Unbind(ForEachBy::new(self, attach, detach))
    }
    pub fn for_each_async<Fut: Future<Output = ()> + 'static>(
        self,
        f: impl Fn(B::Item) -> Fut + 'static,
    ) -> Unbind {
        let sp = get_current_local_spawn();
        self.for_each_by(
            move |value| sp.spawn_local_with_handle(f(value)).unwrap(),
            move |_handle| {},
        )
    }

    pub fn map<U>(self, f: impl Fn(B::Item) -> U + 'static) -> BindExt<impl Reactive<Item = U>> {
        make_reactive(move |ctx| f(self.get(ctx)))
    }
    pub fn map_with_ctx<U>(
        self,
        f: impl Fn(B::Item, &mut ReactiveContext) -> U + 'static,
    ) -> BindExt<impl Reactive<Item = U>> {
        reactive(move |ctx| f(self.get(ctx), ctx))
    }
    pub fn flat_map<O: Reactive>(
        self,
        f: impl Fn(B::Item) -> O + 'static,
    ) -> BindExt<impl Reactive<Item = O::Item>> {
        self.map(f).flatten()
    }
    pub fn flatten(self) -> BindExt<impl Reactive<Item = <B::Item as Reactive>::Item>>
    where
        B::Item: Reactive,
    {
        reactive(move |ctx| self.get(ctx).get(ctx))
    }
    pub fn map_async<Fut: Future + 'static>(
        self,
        f: impl Fn(B::Item) -> Fut + 'static,
    ) -> RefBindExt<impl ReactiveRef<Item = Poll<Fut::Output>>> {
        RefBindExt(MapAsync::new(self.map(f)))
    }
}

/// Extension methods for `ReactiveRef`.
///
/// Since impl trait return value is used, `BindExt` is struct instead of trait.
#[derive(Clone)]
pub struct RefBindExt<B>(pub(crate) B);

impl<B: ReactiveRef> ReactiveRef for RefBindExt<B> {
    type Item = B::Item;
    fn borrow(&self, ctx: &mut ReactiveContext) -> Ref<Self::Item> {
        self.0.borrow(ctx)
    }
}

impl<B: ReactiveRef> RefBindExt<B> {
    pub fn for_each(self, f: impl Fn(&B::Item) + 'static) -> Unbind {
        self.map(f).for_each(|_| {})
    }
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

    pub fn map<U>(self, f: impl Fn(&B::Item) -> U + 'static) -> BindExt<impl Reactive<Item = U>> {
        reactive(move |ctx| f(&self.borrow(ctx)))
    }
    pub fn map_ref<U: 'static>(
        self,
        f: impl Fn(&B::Item) -> &U + 'static,
    ) -> RefBindExt<impl ReactiveRef<Item = U>> {
        reactive_ref(self, move |this, ctx| Ref::map(this.borrow(ctx), &f))
    }
    pub fn map_with_ctx<U>(
        self,
        f: impl Fn(&B::Item, &mut ReactiveContext) -> U + 'static,
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




struct ForEach<B, F> {
    b: B,
    f: F,
    binds: RefCell<Vec<Binding>>,
}

impl<B: Reactive, F: Fn(B::Item) + 'static> ForEach<B, F> {
    fn new(b: B, f: F) -> Rc<Self> {
        let s = Rc::new(ForEach {
            b,
            f,
            binds: RefCell::new(Vec::new()),
        });
        s.next();
        s
    }

    fn next(self: &Rc<Self>) {
        let mut b = self.binds.borrow_mut();
        let mut ctx = ReactiveContext::new(&self, &mut b);
        (self.f)(self.b.get(&mut ctx));
    }
}
impl<B: Reactive, F: Fn(B::Item) + 'static> BindSink for ForEach<B, F> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        self.binds.borrow_mut().clear();
        ctx.spawn(Rc::downgrade(&self))
    }
}
impl<B: Reactive, F: Fn(B::Item) + 'static> Task for ForEach<B, F> {
    fn run(self: Rc<Self>) {
        self.next();
    }
}

struct ForEachBy<B, A, D, T>
where
    B: Reactive,
    A: Fn(B::Item) -> T + 'static,
    D: Fn(T) + 'static,
    T: 'static,
{
    b: B,
    attach: A,
    detach: D,
    value: RefCell<Option<T>>,
    binds: RefCell<Vec<Binding>>,
}

impl<B, A, D, T> ForEachBy<B, A, D, T>
where
    B: Reactive,
    A: Fn(B::Item) -> T + 'static,
    D: Fn(T) + 'static,
    T: 'static,
{
    fn new(b: B, attach: A, detach: D) -> Rc<Self> {
        let s = Rc::new(ForEachBy {
            b,
            attach,
            detach,
            value: RefCell::new(None),
            binds: RefCell::new(Vec::new()),
        });
        s.next();
        s
    }

    fn next(self: &Rc<Self>) {
        let mut b = self.binds.borrow_mut();
        let mut ctx = ReactiveContext::new(&self, &mut b);
        *self.value.borrow_mut() = Some((self.attach)(self.b.get(&mut ctx)));
    }
    fn detach_value(&self) {
        if let Some(value) = self.value.borrow_mut().take() {
            (self.detach)(value);
        }
    }
}
impl<B, A, D, T> BindSink for ForEachBy<B, A, D, T>
where
    B: Reactive,
    A: Fn(B::Item) -> T + 'static,
    D: Fn(T) + 'static,
    T: 'static,
{
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        self.binds.borrow_mut().clear();
        self.detach_value();
        ctx.spawn(Rc::downgrade(&self))
    }
}
impl<B, A, D, T> Task for ForEachBy<B, A, D, T>
where
    B: Reactive,
    A: Fn(B::Item) -> T + 'static,
    D: Fn(T) + 'static,
    T: 'static,
{
    fn run(self: Rc<Self>) {
        self.next();
    }
}
impl<B, A, D, T> Drop for ForEachBy<B, A, D, T>
where
    B: Reactive,
    A: Fn(B::Item) -> T + 'static,
    D: Fn(T) + 'static,
    T: 'static,
{
    fn drop(&mut self) {
        self.detach_value();
    }
}

struct MapAsync<B>(Rc<MapAsyncData<B>>)
where
    B: Reactive,
    B::Item: Future + 'static,
    <B::Item as Future>::Output: 'static;

struct MapAsyncData<B>
where
    B: Reactive,
    B::Item: Future + 'static,
    <B::Item as Future>::Output: 'static,
{
    sp: Rc<dyn LocalSpawn>,
    b: B,
    sinks: BindSinks,
    state: RefCell<MapAsyncState<<B::Item as Future>::Output>>,
}
struct MapAsyncState<T> {
    value: Poll<T>,
    handle: Option<RemoteHandle<()>>,
    binds: Vec<Binding>,
}

impl<B> MapAsync<B>
where
    B: Reactive,
    B::Item: Future + 'static,
    <B::Item as Future>::Output: 'static,
{
    fn new(b: B) -> Self {
        MapAsync(Rc::new(MapAsyncData {
            sp: get_current_local_spawn(),
            b,
            sinks: BindSinks::new(),
            state: RefCell::new(MapAsyncState {
                value: Poll::Pending,
                handle: None,
                binds: Vec::new(),
            }),
        }))
    }
}
impl<B> MapAsyncData<B>
where
    B: Reactive,
    B::Item: Future + 'static,
    <B::Item as Future>::Output: 'static,
{
    fn ready(self: &Rc<Self>) {
        let mut s = self.state.borrow_mut();
        let mut ctx = ReactiveContext::new(self, &mut s.binds);
        let fut = self.b.get(&mut ctx);
        let this = Rc::downgrade(self);
        s.handle = Some(
            self.sp
                .spawn_local_with_handle(async move {
                    let value = fut.await;
                    if let Some(this) = Weak::upgrade(&this) {
                        let mut s = this.state.borrow_mut();
                        s.value = Poll::Ready(value);
                        drop(s);
                        this.sinks.notify();
                    }
                })
                .unwrap(),
        );
    }
    fn borrow<'a>(
        self: &'a Rc<Self>,
        ctx: &mut ReactiveContext,
    ) -> Ref<'a, Poll<<B::Item as Future>::Output>> {
        let mut s = self.state.borrow();
        if s.handle.is_none() {
            drop(s);
            self.ready();
            s = self.state.borrow();
        }
        ctx.bind(self.clone());
        Ref::map(Ref::Cell(s), |o| &o.value)
    }
}

impl<B> ReactiveRef for MapAsync<B>
where
    B: Reactive,
    B::Item: Future + 'static,
    <B::Item as Future>::Output: 'static,
{
    type Item = Poll<<B::Item as Future>::Output>;

    fn borrow(&self, ctx: &mut ReactiveContext) -> Ref<Self::Item> {
        self.0.borrow(ctx)
    }
    fn into_rc(self) -> RcReRef<Self::Item> {
        self.0
    }
}
impl<B> InnerReactiveRef for MapAsyncData<B>
where
    B: Reactive,
    B::Item: Future + 'static,
    <B::Item as Future>::Output: 'static,
{
    type Item = Poll<<B::Item as Future>::Output>;

    fn rc_borrow<'a>(
        &'a self,
        rc_self: &'a dyn Any,
        ctx: &mut ReactiveContext,
    ) -> Ref<'a, Self::Item> {
        Self::downcast(rc_self).borrow(ctx)
    }
}

impl<B> BindSource for MapAsyncData<B>
where
    B: Reactive,
    B::Item: Future + 'static,
    <B::Item as Future>::Output: 'static,
{
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
impl<B> BindSink for MapAsyncData<B>
where
    B: Reactive,
    B::Item: Future + 'static,
    <B::Item as Future>::Output: 'static,
{
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        let mut s = self.state.borrow_mut();
        if s.handle.is_some() {
            s.handle = None;
            if let Poll::Ready(_) = &s.value {
                s.value = Poll::Pending;
                drop(s);
                self.sinks.notify_with(ctx);
            }
        }
    }
}

pub fn constant<T: 'static>(value: T) -> RefBindExt<impl ReactiveRef<Item = T>> {
    struct Constant<T: 'static>(T);
    impl<T> ReactiveRef for Constant<T> {
        type Item = T;
        fn borrow(&self, _: &mut ReactiveContext) -> Ref<Self::Item> {
            Ref::Native(&self.0)
        }
    }
    RefBindExt(Constant(value))
}

*/
