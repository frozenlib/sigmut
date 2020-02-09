use futures::future::RemoteHandle;
use futures::task::{LocalSpawn, LocalSpawnExt};
use std::any::Any;
use std::cell::RefCell;
use std::cmp::min;
use std::future::Future;
use std::mem::drop;
use std::mem::replace;
use std::ops::Deref;
use std::rc::{Rc, Weak};
use std::task::Poll;

pub trait BindSource: 'static {
    fn sinks(&self) -> &BindSinks;
    fn bind(self: Rc<Self>, sink: Weak<dyn BindSink>) -> Binding
    where
        Self: Sized,
    {
        let idx = self.sinks().0.borrow_mut().insert(sink.clone());
        Binding {
            source: self,
            sink,
            idx,
        }
    }
}

pub trait BindSink: 'static {
    fn notify(self: Rc<Self>, ctx: &NotifyContext);
}

pub struct Binding {
    source: Rc<dyn BindSource>,
    sink: Weak<dyn BindSink>,
    idx: usize,
}
impl Drop for Binding {
    fn drop(&mut self) {
        self.source
            .sinks()
            .0
            .borrow_mut()
            .remove(self.idx, &self.sink);
    }
}

pub struct BindSinks(RefCell<BindSinkData>);
impl BindSinks {
    pub fn new() -> Self {
        Self(RefCell::new(BindSinkData::new()))
    }
    pub fn notify_with(&self, ctx: &NotifyContext) {
        self.0.borrow_mut().notify(ctx);
    }
    pub fn notify(&self) {
        NotifyContext::with(|ctx| self.notify_with(ctx));
    }
    pub fn is_empty(&self) -> bool {
        self.0
            .borrow_mut()
            .sinks
            .iter()
            .all(|x| x.strong_count() == 0)
    }
}

struct BindSinkData {
    sinks: Vec<Weak<dyn BindSink>>,
    idx_next: usize,
}
impl BindSinkData {
    fn new() -> Self {
        Self {
            sinks: Vec::new(),
            idx_next: 0,
        }
    }
    fn insert(&mut self, sink: Weak<dyn BindSink>) -> usize {
        while self.idx_next < self.sinks.len() {
            if self.sinks[self.idx_next].strong_count() == 0 {
                let idx = self.idx_next;
                self.sinks[idx] = sink;
                self.idx_next += 1;
                return idx;
            }
        }
        let idx = self.sinks.len();
        self.sinks.push(sink);
        idx
    }
    fn remove(&mut self, idx: usize, sink: &Weak<dyn BindSink>) {
        if let Some(s) = self.sinks.get_mut(idx) {
            if Weak::ptr_eq(s, sink) {
                *s = freed_sink();
                self.idx_next = min(self.idx_next, idx);
            }
        }
    }

    fn notify(&mut self, ctx: &NotifyContext) {
        for s in &mut self.sinks {
            if let Some(sink) = Weak::upgrade(&replace(s, freed_sink())) {
                sink.notify(ctx);
            }
        }
        self.sinks.clear();
        self.idx_next = 0;
    }
}
fn freed_sink() -> Weak<dyn BindSink> {
    struct DummyBindSink;
    impl BindSink for DummyBindSink {
        fn notify(self: Rc<Self>, _ctx: &NotifyContext) {}
    }
    Weak::<DummyBindSink>::new()
}

pub struct NotifyContext(RefCell<NotifyContextData>);
struct NotifyContextData {
    ref_count: usize,
    tasks: Vec<Weak<dyn Task>>,
}

pub trait Task: 'static {
    fn run(self: Rc<Self>);
}

impl NotifyContext {
    fn new() -> Self {
        Self(RefCell::new(NotifyContextData {
            ref_count: 0,
            tasks: Vec::new(),
        }))
    }

    pub fn with(f: impl Fn(&NotifyContext)) {
        thread_local!(static CTX: NotifyContext = NotifyContext::new());
        CTX.with(|ctx| {
            ctx.enter();
            f(ctx);
            ctx.leave();
        });
    }
    fn enter(&self) {
        let mut d = self.0.borrow_mut();
        assert!(d.ref_count != usize::max_value());
        d.ref_count += 1;
    }
    fn leave(&self) {
        let mut d = self.0.borrow_mut();
        assert!(d.ref_count != 0);
        if d.ref_count == 1 {
            while let Some(task) = d.tasks.pop() {
                if let Some(task) = Weak::upgrade(&task) {
                    drop(d);
                    task.run();
                    d = self.0.borrow_mut();
                }
            }
        }
        d.ref_count -= 1;
    }

    pub fn spawn(&self, task: Weak<impl Task>) {
        self.0.borrow_mut().tasks.push(task);
    }
}

pub struct BindContext<'a> {
    sink: Weak<dyn BindSink>,
    bindings: &'a mut Vec<Binding>,
}
impl<'a> BindContext<'a> {
    pub fn new(sink: &Rc<impl BindSink + 'static>, bindings: &'a mut Vec<Binding>) -> Self {
        Self {
            sink: Rc::downgrade(sink) as Weak<dyn BindSink>,
            bindings,
        }
    }
    pub fn bind(&mut self, src: Rc<impl BindSource>) {
        self.bindings.push(src.bind(self.sink.clone()));
    }
}

pub trait Bind: Sized + 'static {
    type Item;

    fn bind(&self, ctx: &mut BindContext) -> Self::Item;
}

#[derive(Clone)]
pub struct BindExt<B>(pub B);

impl<B: Bind> Bind for BindExt<B> {
    type Item = B::Item;
    fn bind(&self, ctx: &mut BindContext) -> Self::Item {
        self.0.bind(ctx)
    }
}

impl<B: Bind> BindExt<B> {
    pub fn from(b: B) -> Self {
        Self(b)
    }
    pub fn cached(self) -> RefBindExt<impl RefBind<Item = B::Item>> {
        RefBindExt(Cached::new(self))
    }
    pub fn dedup(self) -> RefBindExt<impl RefBind<Item = B::Item>>
    where
        B::Item: PartialEq,
    {
        RefBindExt(Dedup::new(self))
    }

    pub fn for_each(self, f: impl Fn(B::Item) + 'static) -> Unbind {
        Unbind(ForEachData::new(self, f))
    }

    pub fn map<U>(self, f: impl Fn(B::Item) -> U + 'static) -> BindExt<impl Bind<Item = U>> {
        BindExt(Map { b: self, f })
    }
    pub fn map_async<Fut: Future + 'static>(
        self,
        f: impl Fn(B::Item) -> Fut + 'static,
    ) -> RefBindExt<impl RefBind<Item = Poll<Fut::Output>>> {
        RefBindExt(MapAsync::new(self, f))
    }
}

pub trait RefBind: Sized + 'static {
    type Item;

    fn bind(&self, ctx: &mut BindContext) -> Ref<Self::Item>;
}

#[derive(Clone)]
pub struct RefBindExt<B>(pub B);

impl<B: RefBind> RefBind for RefBindExt<B> {
    type Item = B::Item;
    fn bind(&self, ctx: &mut BindContext) -> Ref<Self::Item> {
        self.0.bind(ctx)
    }
}

impl<B: RefBind> RefBindExt<B> {
    pub fn from(b: B) -> Self {
        Self(b)
    }

    pub fn for_each(self, f: impl Fn(&B::Item) + 'static) -> Unbind {
        Unbind(RefForEachData::new(self, f))
    }

    pub fn map<U>(self, f: impl Fn(&B::Item) -> U + 'static) -> BindExt<impl Bind<Item = U>> {
        BindExt(RefMap { b: self, f })
    }
    pub fn map_ref<U>(
        self,
        f: impl Fn(&B::Item) -> &U + 'static,
    ) -> RefBindExt<impl RefBind<Item = U>> {
        RefBindExt(RefMapRef { b: self, f })
    }
    pub fn cloned(self) -> BindExt<impl Bind<Item = B::Item>>
    where
        B::Item: Clone,
    {
        self.map(|x| x.clone())
    }
}

pub enum Ref<'a, T> {
    Native(&'a T),
    Cell(std::cell::Ref<'a, T>),
}
impl<'a, T> Ref<'a, T> {
    pub fn map<U>(this: Self, f: impl FnOnce(&T) -> &U) -> Ref<'a, U> {
        use Ref::*;
        match this {
            Native(x) => Native(f(x)),
            Cell(x) => Cell(std::cell::Ref::map(x, f)),
        }
    }
}
impl<'a, T> Deref for Ref<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        match self {
            Ref::Native(x) => x,
            Ref::Cell(x) => x,
        }
    }
}
pub struct Unbind(Rc<dyn Any>);

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

    fn ready(&self) {
        let mut s = self.0.state.borrow_mut();
        let mut ctx = BindContext::new(&self.0, &mut s.binds);
        s.value = Some(self.0.b.bind(&mut ctx));
    }
}
impl<B: Bind> RefBind for Cached<B> {
    type Item = B::Item;
    fn bind(&self, ctx: &mut BindContext) -> Ref<Self::Item> {
        ctx.bind(self.0.clone());
        let mut s = self.0.state.borrow();
        if s.value.is_none() {
            drop(s);
            self.ready();
            s = self.0.state.borrow();
        }
        return Ref::map(Ref::Cell(s), |o| o.value.as_ref().unwrap());
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
struct Dedup<B: Bind>(Rc<DedupData<B>>);

struct DedupData<B: Bind> {
    b: B,
    sinks: BindSinks,
    state: RefCell<DedupState<B::Item>>,
}
struct DedupState<T> {
    value: Option<T>,
    is_ready: bool,
    binds: Vec<Binding>,
}
impl<B: Bind> Dedup<B>
where
    B::Item: PartialEq,
{
    fn new(b: B) -> Self {
        Self(Rc::new(DedupData {
            b,
            sinks: BindSinks::new(),
            state: RefCell::new(DedupState {
                value: None,
                is_ready: false,
                binds: Vec::new(),
            }),
        }))
    }
}
impl<B: Bind> RefBind for Dedup<B>
where
    B::Item: PartialEq,
{
    type Item = B::Item;
    fn bind(&self, ctx: &mut BindContext) -> Ref<Self::Item> {
        let mut s = self.0.state.borrow();
        if s.is_ready {
            drop(s);
            self.0.ready();
            s = self.0.state.borrow();
        }
        ctx.bind(self.0.clone());
        return Ref::map(Ref::Cell(s), |o| o.value.as_ref().unwrap());
    }
}
impl<B: Bind> BindSource for DedupData<B>
where
    B::Item: PartialEq,
{
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
impl<B: Bind> BindSink for DedupData<B>
where
    B::Item: PartialEq,
{
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
impl<B: Bind> DedupData<B>
where
    B::Item: PartialEq,
{
    fn ready(self: &Rc<Self>) {
        let mut s = self.state.borrow_mut();
        let mut ctx = BindContext::new(&self, &mut s.binds);
        let value = self.b.bind(&mut ctx);
        if s.value.as_ref() != Some(&value) {
            s.value = Some(value);
            drop(s);
            self.sinks.notify();
        }
    }
}
impl<B: Bind> Task for DedupData<B>
where
    B::Item: PartialEq,
{
    fn run(self: Rc<Self>) {
        self.ready();
    }
}

struct ForEachData<B, F> {
    b: B,
    f: F,
    binds: RefCell<Vec<Binding>>,
}

impl<B: Bind, F: Fn(B::Item) + 'static> ForEachData<B, F> {
    fn new(b: B, f: F) -> Rc<Self> {
        let s = Rc::new(Self {
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
        (self.f)(self.b.bind(&mut ctx));
    }
}
impl<B: Bind, F: Fn(B::Item) + 'static> BindSink for ForEachData<B, F> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        ctx.spawn(Rc::downgrade(&self))
    }
}
impl<B: Bind, F: Fn(B::Item) + 'static> Task for ForEachData<B, F> {
    fn run(self: Rc<Self>) {
        self.next();
    }
}

struct RefForEachData<B, F> {
    b: B,
    f: F,
    binds: RefCell<Vec<Binding>>,
}

impl<B: RefBind, F: Fn(&B::Item) + 'static> RefForEachData<B, F> {
    fn new(b: B, f: F) -> Rc<Self> {
        let s = Rc::new(Self {
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
        (self.f)(&self.b.bind(&mut ctx));
    }
}
impl<B: RefBind, F: Fn(&B::Item) + 'static> BindSink for RefForEachData<B, F> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        ctx.spawn(Rc::downgrade(&self))
    }
}
impl<B: RefBind, F: Fn(&B::Item) + 'static> Task for RefForEachData<B, F> {
    fn run(self: Rc<Self>) {
        self.next();
    }
}

struct Map<B, F> {
    b: B,
    f: F,
}
impl<B: Bind, F: Fn(B::Item) -> U + 'static, U> Bind for Map<B, F> {
    type Item = U;

    fn bind(&self, ctx: &mut BindContext) -> Self::Item {
        (self.f)(self.b.bind(ctx))
    }
}

struct RefMap<B, F> {
    b: B,
    f: F,
}
impl<B: RefBind, F: Fn(&B::Item) -> U + 'static, U> Bind for RefMap<B, F> {
    type Item = U;

    fn bind(&self, ctx: &mut BindContext) -> Self::Item {
        (self.f)(&self.b.bind(ctx))
    }
}

struct RefMapRef<B, F> {
    b: B,
    f: F,
}
impl<B: RefBind, F: Fn(&B::Item) -> &U + 'static, U> RefBind for RefMapRef<B, F> {
    type Item = U;

    fn bind(&self, ctx: &mut BindContext) -> Ref<Self::Item> {
        Ref::map(self.b.bind(ctx), &self.f)
    }
}

struct MapAsync<B: Bind, F: Fn(B::Item) -> Fut, Fut: Future>(Rc<MapAsyncData<B, F, Fut>>);

struct MapAsyncData<B: Bind, F: Fn(B::Item) -> Fut, Fut: Future> {
    sp: Rc<dyn LocalSpawn>,
    b: B,
    f: F,
    sinks: BindSinks,
    state: RefCell<MapAsyncState<Fut::Output>>,
}
struct MapAsyncState<T> {
    value: Poll<T>,
    handle: Option<RemoteHandle<()>>,
    binds: Vec<Binding>,
}

impl<B: Bind, F: Fn(B::Item) -> Fut + 'static, Fut: Future<Output = U> + 'static, U>
    MapAsync<B, F, Fut>
{
    fn new(b: B, f: F) -> Self {
        MapAsync(Rc::new(MapAsyncData {
            sp: get_current_local_spawn(),
            b,
            f,
            sinks: BindSinks::new(),
            state: RefCell::new(MapAsyncState {
                value: Poll::Pending,
                handle: None,
                binds: Vec::new(),
            }),
        }))
    }

    fn ready(&self) {
        let mut s = self.0.state.borrow_mut();
        let mut ctx = BindContext::new(&self.0, &mut s.binds);
        let value = self.0.b.bind(&mut ctx);
        let fut = (self.0.f)(value);
        let this = Rc::downgrade(&self.0);
        s.handle = Some(
            self.0
                .sp
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
}

impl<B: Bind, F: Fn(B::Item) -> Fut + 'static, Fut: Future<Output = U> + 'static, U> RefBind
    for MapAsync<B, F, Fut>
{
    type Item = Poll<U>;

    fn bind(&self, ctx: &mut BindContext) -> Ref<Self::Item> {
        let mut s = self.0.state.borrow();
        if s.handle.is_none() {
            drop(s);
            self.ready();
            s = self.0.state.borrow();
        }
        ctx.bind(self.0.clone());
        Ref::map(Ref::Cell(s), |o| &o.value)
    }
}
impl<B: Bind, F: Fn(B::Item) -> Fut + 'static, Fut: Future<Output = U> + 'static, U> BindSource
    for MapAsyncData<B, F, Fut>
{
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
impl<B: Bind, F: Fn(B::Item) -> Fut + 'static, Fut: Future<Output = U> + 'static, U> BindSink
    for MapAsyncData<B, F, Fut>
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
    RefBindExt(Constant(value))
}

#[derive(Clone)]
struct Constant<T: 'static>(T);

impl<T> RefBind for Constant<T> {
    type Item = T;
    fn bind(&self, _: &mut BindContext) -> Ref<Self::Item> {
        Ref::Native(&self.0)
    }
}

impl<F: Fn(&BindContext) -> T + 'static, T> Bind for F {
    type Item = T;
    fn bind(&self, ctx: &mut BindContext) -> Self::Item {
        self(ctx)
    }
}

impl<F: Fn(&BindContext) -> &'static T + 'static, T: 'static> RefBind for F {
    type Item = T;
    fn bind(&self, ctx: &mut BindContext) -> Ref<Self::Item> {
        Ref::Native(self(ctx))
    }
}

thread_local! {
    pub static LOCAL_SPAWN: RefCell<Rc<dyn LocalSpawn>> = RefCell::new(Rc::new(LocalSpawnNotSet));
}
struct LocalSpawnNotSet;
impl LocalSpawn for LocalSpawnNotSet {
    fn spawn_local_obj(
        &self,
        _: futures::task::LocalFutureObj<'static, ()>,
    ) -> Result<(), futures::task::SpawnError> {
        panic!("need to call `set_current_local_spawn`.");
    }
}

pub fn set_current_local_spawn(sp: impl LocalSpawn + 'static) {
    LOCAL_SPAWN.with(|value| *value.borrow_mut() = Rc::new(sp));
}
pub fn get_current_local_spawn() -> Rc<dyn LocalSpawn + 'static> {
    LOCAL_SPAWN.with(|value| value.borrow().clone())
}
