use crate::*;
use futures::future::RemoteHandle;
use futures::task::{LocalSpawn, LocalSpawnExt};
use std::any::Any;
use std::cell::RefCell;
use std::future::Future;
use std::mem::drop;
use std::rc::{Rc, Weak};
use std::task::Poll;

/// Extension methods for `Bind`.
///
/// Since impl trait return value is used, `BindExt` is struct instead of trait.
#[derive(Clone)]
pub struct BindExt<B>(pub(crate) B);

impl<B: Bind> Bind for BindExt<B> {
    type Item = B::Item;
    fn get(&self, ctx: &mut BindContext) -> Self::Item {
        self.0.get(ctx)
    }
}

impl<B: Bind> BindExt<B> {
    pub fn cached(self) -> RefBindExt<impl RefBind<Item = B::Item>> {
        RefBindExt(Cached::new(self))
    }
    pub fn dedup_by(
        self,
        eq: impl Fn(&B::Item, &B::Item) -> bool + 'static,
    ) -> RefBindExt<impl RefBind<Item = B::Item>> {
        RefBindExt(DedupBy::new(self, eq))
    }
    pub fn dedup_by_key<K: PartialEq>(
        self,
        to_key: impl Fn(&B::Item) -> K + 'static,
    ) -> RefBindExt<impl RefBind<Item = B::Item>> {
        self.dedup_by(move |l, r| to_key(l) == to_key(r))
    }

    pub fn dedup(self) -> RefBindExt<impl RefBind<Item = B::Item>>
    where
        B::Item: PartialEq,
    {
        self.dedup_by(|l, r| l == r)
    }

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

    pub fn map<U>(self, f: impl Fn(B::Item) -> U + 'static) -> BindExt<impl Bind<Item = U>> {
        make_bind(move |ctx| f(self.get(ctx)))
    }
    pub fn map_with_ctx<U>(
        self,
        f: impl Fn(B::Item, &mut BindContext) -> U + 'static,
    ) -> BindExt<impl Bind<Item = U>> {
        make_bind(move |ctx| f(self.get(ctx), ctx))
    }
    pub fn flat_map<O: Bind>(
        self,
        f: impl Fn(B::Item) -> O + 'static,
    ) -> BindExt<impl Bind<Item = O::Item>> {
        self.map(f).flatten()
    }
    pub fn flatten(self) -> BindExt<impl Bind<Item = <B::Item as Bind>::Item>>
    where
        B::Item: Bind,
    {
        make_bind(move |ctx| self.get(ctx).get(ctx))
    }
    pub fn map_async<Fut: Future + 'static>(
        self,
        f: impl Fn(B::Item) -> Fut + 'static,
    ) -> RefBindExt<impl RefBind<Item = Poll<Fut::Output>>> {
        RefBindExt(MapAsync::new(self.map(f)))
    }
}

/// Extension methods for `RefBind`.
///
/// Since impl trait return value is used, `BindExt` is struct instead of trait.
#[derive(Clone)]
pub struct RefBindExt<B>(pub(crate) B);

impl<B: RefBind> RefBind for RefBindExt<B> {
    type Item = B::Item;
    fn borrow(&self, ctx: &mut BindContext) -> Ref<Self::Item> {
        self.0.borrow(ctx)
    }
}

impl<B: RefBind> RefBindExt<B> {
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

    pub fn map<U>(self, f: impl Fn(&B::Item) -> U + 'static) -> BindExt<impl Bind<Item = U>> {
        make_bind(move |ctx| f(&self.borrow(ctx)))
    }
    pub fn map_ref<U: 'static>(
        self,
        f: impl Fn(&B::Item) -> &U + 'static,
    ) -> RefBindExt<impl RefBind<Item = U>> {
        make_ref_bind(self, move |this, ctx| Ref::map(this.borrow(ctx), &f))
    }
    pub fn map_with_ctx<U>(
        self,
        f: impl Fn(&B::Item, &mut BindContext) -> U + 'static,
    ) -> BindExt<impl Bind<Item = U>> {
        make_bind(move |ctx| f(&self.borrow(ctx), ctx))
    }
    pub fn flat_map<O: Bind>(
        self,
        f: impl Fn(&B::Item) -> O + 'static,
    ) -> BindExt<impl Bind<Item = O::Item>> {
        self.map(f).flatten()
    }
    pub fn map_async<Fut: Future + 'static>(
        self,
        f: impl Fn(&B::Item) -> Fut + 'static,
    ) -> RefBindExt<impl RefBind<Item = Poll<Fut::Output>>> {
        RefBindExt(MapAsync::new(self.map(f)))
    }

    pub fn cloned(self) -> BindExt<impl Bind<Item = B::Item>>
    where
        B::Item: Clone,
    {
        self.map(|x| x.clone())
    }
}

#[derive(Clone)]
struct Cached<B: Bind>(Rc<CachedData<B>>);
struct CachedData<B: Bind> {
    b: B,
    sinks: BindSinks,
    state: RefCell<CachedState<B::Item>>,
}
struct CachedState<T> {
    value: Option<T>,
    binds: Vec<Binding>,
}

impl<B: Bind> Cached<B> {
    fn new(b: B) -> Self {
        Self(Rc::new(CachedData {
            b,
            sinks: BindSinks::new(),
            state: RefCell::new(CachedState {
                value: None,
                binds: Vec::new(),
            }),
        }))
    }
}
impl<B: Bind> CachedData<B> {
    fn ready(self: &Rc<Self>) {
        let mut s = self.state.borrow_mut();
        let mut ctx = BindContext::new(&self, &mut s.binds);
        s.value = Some(self.b.get(&mut ctx));
    }
    fn borrow<'a>(self: &'a Rc<Self>, ctx: &mut BindContext) -> Ref<'a, B::Item> {
        ctx.bind(self.clone());
        let mut s = self.state.borrow();
        if s.value.is_none() {
            drop(s);
            self.ready();
            s = self.state.borrow();
        }
        return Ref::map(Ref::Cell(s), |o| o.value.as_ref().unwrap());
    }
}
impl<B: Bind> RefBind for Cached<B> {
    type Item = B::Item;
    fn borrow(&self, ctx: &mut BindContext) -> Ref<Self::Item> {
        self.0.borrow(ctx)
    }
    fn into_rc(self) -> RcRefBind<Self::Item> {
        self.0
    }
}
impl<B: Bind> DynRefBind for CachedData<B> {
    type Item = B::Item;

    fn dyn_borrow<'a>(
        &'a self,
        rc_this: &'a dyn Any,
        ctx: &mut BindContext,
    ) -> Ref<'a, Self::Item> {
        Self::downcast(rc_this).borrow(ctx)
    }
}
impl<B: Bind> BindSource for CachedData<B> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
impl<B: Bind> BindSink for CachedData<B> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        let mut s = self.state.borrow_mut();
        if s.value.is_some() {
            s.value = None;
            s.binds.clear();
            self.sinks.notify_with(ctx);
        }
    }
}

#[derive(Clone)]
struct DedupBy<B: Bind, EqFn>(Rc<DedupByData<B, EqFn>>);

struct DedupByData<B: Bind, EqFn> {
    b: B,
    eq: EqFn,
    sinks: BindSinks,
    state: RefCell<DedupByState<B::Item>>,
}
struct DedupByState<T> {
    value: Option<T>,
    is_ready: bool,
    binds: Vec<Binding>,
}
impl<B: Bind, EqFn> DedupBy<B, EqFn> {
    fn new(b: B, eq: EqFn) -> Self {
        Self(Rc::new(DedupByData {
            b,
            eq,
            sinks: BindSinks::new(),
            state: RefCell::new(DedupByState {
                value: None,
                is_ready: false,
                binds: Vec::new(),
            }),
        }))
    }
}
impl<B: Bind, EqFn: Fn(&B::Item, &B::Item) -> bool + 'static> DedupByData<B, EqFn> {
    fn ready(self: &Rc<Self>) {
        let mut s = self.state.borrow_mut();
        let mut ctx = BindContext::new(&self, &mut s.binds);
        let value = self.b.get(&mut ctx);
        if let Some(value_old) = &s.value {
            if (self.eq)(value_old, &value) {
                return;
            }
        }
        s.value = Some(value);
        drop(s);
        self.sinks.notify();
    }
    fn borrow<'a>(self: &'a Rc<Self>, ctx: &mut BindContext) -> Ref<'a, B::Item> {
        let mut s = self.state.borrow();
        if s.is_ready {
            drop(s);
            self.ready();
            s = self.state.borrow();
        }
        ctx.bind(self.clone());
        return Ref::map(Ref::Cell(s), |o| o.value.as_ref().unwrap());
    }
}
impl<B: Bind, EqFn: Fn(&B::Item, &B::Item) -> bool + 'static> RefBind for DedupBy<B, EqFn> {
    type Item = B::Item;
    fn borrow(&self, ctx: &mut BindContext) -> Ref<Self::Item> {
        self.0.borrow(ctx)
    }
    fn into_rc(self) -> RcRefBind<Self::Item> {
        self.0
    }
}
impl<B: Bind, EqFn: Fn(&B::Item, &B::Item) -> bool + 'static> DynRefBind for DedupByData<B, EqFn> {
    type Item = B::Item;

    fn dyn_borrow<'a>(
        &'a self,
        rc_this: &'a dyn Any,
        ctx: &mut BindContext,
    ) -> Ref<'a, Self::Item> {
        Self::downcast(rc_this).borrow(ctx)
    }
}
impl<B: Bind, EqFn: Fn(&B::Item, &B::Item) -> bool + 'static> BindSource for DedupByData<B, EqFn> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
impl<B: Bind, EqFn: Fn(&B::Item, &B::Item) -> bool + 'static> BindSink for DedupByData<B, EqFn> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        let mut s = self.state.borrow_mut();
        if s.is_ready {
            s.is_ready = false;
            s.binds.clear();
            if !self.sinks.is_empty() {
                ctx.spawn(Rc::downgrade(&self));
            }
        }
    }
}
impl<B: Bind, EqFn: Fn(&B::Item, &B::Item) -> bool + 'static> Task for DedupByData<B, EqFn> {
    fn run(self: Rc<Self>) {
        self.ready();
    }
}

struct ForEach<B, F> {
    b: B,
    f: F,
    binds: RefCell<Vec<Binding>>,
}

impl<B: Bind, F: Fn(B::Item) + 'static> ForEach<B, F> {
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
        let mut ctx = BindContext::new(&self, &mut b);
        (self.f)(self.b.get(&mut ctx));
    }
}
impl<B: Bind, F: Fn(B::Item) + 'static> BindSink for ForEach<B, F> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        self.binds.borrow_mut().clear();
        ctx.spawn(Rc::downgrade(&self))
    }
}
impl<B: Bind, F: Fn(B::Item) + 'static> Task for ForEach<B, F> {
    fn run(self: Rc<Self>) {
        self.next();
    }
}

struct ForEachBy<B, A, D, T>
where
    B: Bind,
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
    B: Bind,
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
        let mut ctx = BindContext::new(&self, &mut b);
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
    B: Bind,
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
    B: Bind,
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
    B: Bind,
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
    B: Bind,
    B::Item: Future + 'static,
    <B::Item as Future>::Output: 'static;

struct MapAsyncData<B>
where
    B: Bind,
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
    B: Bind,
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
    B: Bind,
    B::Item: Future + 'static,
    <B::Item as Future>::Output: 'static,
{
    fn ready(self: &Rc<Self>) {
        let mut s = self.state.borrow_mut();
        let mut ctx = BindContext::new(self, &mut s.binds);
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
        ctx: &mut BindContext,
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

impl<B> RefBind for MapAsync<B>
where
    B: Bind,
    B::Item: Future + 'static,
    <B::Item as Future>::Output: 'static,
{
    type Item = Poll<<B::Item as Future>::Output>;

    fn borrow(&self, ctx: &mut BindContext) -> Ref<Self::Item> {
        self.0.borrow(ctx)
    }
    fn into_rc(self) -> RcRefBind<Self::Item> {
        self.0
    }
}
impl<B> DynRefBind for MapAsyncData<B>
where
    B: Bind,
    B::Item: Future + 'static,
    <B::Item as Future>::Output: 'static,
{
    type Item = Poll<<B::Item as Future>::Output>;

    fn dyn_borrow<'a>(
        &'a self,
        rc_this: &'a dyn Any,
        ctx: &mut BindContext,
    ) -> Ref<'a, Self::Item> {
        Self::downcast(rc_this).borrow(ctx)
    }
}

impl<B> BindSource for MapAsyncData<B>
where
    B: Bind,
    B::Item: Future + 'static,
    <B::Item as Future>::Output: 'static,
{
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
impl<B> BindSink for MapAsyncData<B>
where
    B: Bind,
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

pub fn constant<T: 'static>(value: T) -> RefBindExt<impl RefBind<Item = T>> {
    struct Constant<T: 'static>(T);
    impl<T> RefBind for Constant<T> {
        type Item = T;
        fn borrow(&self, _: &mut BindContext) -> Ref<Self::Item> {
            Ref::Native(&self.0)
        }
    }
    RefBindExt(Constant(value))
}

pub fn make_bind<T>(f: impl Fn(&mut BindContext) -> T + 'static) -> BindExt<impl Bind<Item = T>> {
    struct FnBind<F>(F);
    impl<F: Fn(&mut BindContext) -> T + 'static, T> Bind for FnBind<F> {
        type Item = T;
        fn get(&self, ctx: &mut BindContext) -> Self::Item {
            (self.0)(ctx)
        }
    }
    BindExt(FnBind(f))
}

pub fn make_ref_bind<T, F, U>(this: T, f: F) -> RefBindExt<impl RefBind<Item = U>>
where
    T: 'static,
    for<'a> F: Fn(&'a T, &mut BindContext) -> Ref<'a, U> + 'static,
    U: 'static,
{
    struct FnRefBind<T, F> {
        this: T,
        f: F,
    }
    impl<T, F, U> RefBind for FnRefBind<T, F>
    where
        T: 'static,
        for<'a> F: Fn(&'a T, &mut BindContext) -> Ref<'a, U> + 'static,
        U: 'static,
    {
        type Item = U;
        fn borrow(&self, ctx: &mut BindContext) -> Ref<U> {
            (self.f)(&self.this, ctx)
        }
    }

    RefBindExt(FnRefBind { this, f })
}
