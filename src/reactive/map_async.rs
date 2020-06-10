use crate::bind::*;
use crate::reactive::*;
use futures::Future;
use std::{
    any::Any,
    cell::Ref,
    cell::RefCell,
    rc::{Rc, Weak},
    task::Poll,
};

pub struct MapAsync<S, Fut, Sp>
where
    S: Reactive<Item = Fut>,
    Fut: Future + 'static,
    Sp: LocalSpawn,
{
    sp: Sp,
    source: S,
    sinks: BindSinks,
    state: RefCell<MapAsyncState<Fut::Output, Sp::Handle>>,
}
struct MapAsyncState<T, H> {
    value: Poll<T>,
    handle: Option<H>,
    bindings: Bindings,
}

impl<S, Fut, Sp> MapAsync<S, Fut, Sp>
where
    S: Reactive<Item = Fut>,
    Fut: Future + 'static,
    Sp: LocalSpawn,
{
    pub fn new(source: S, sp: Sp) -> Self {
        Self {
            sp,
            source,
            sinks: BindSinks::new(),
            state: RefCell::new(MapAsyncState {
                value: Poll::Pending,
                handle: None,
                bindings: Bindings::new(),
            }),
        }
    }

    fn ready(self: &Rc<Self>, scope: &BindContextScope) {
        let mut s = self.state.borrow_mut();
        let fut = s.bindings.update(scope, self, |ctx| self.source.get(ctx));
        let this = Rc::downgrade(self);
        s.handle = Some(self.sp.spawn_local(async move {
            let value = fut.await;
            if let Some(this) = Weak::upgrade(&this) {
                this.state.borrow_mut().value = Poll::Ready(value);
                NotifyContext::update(&this);
            }
        }));
    }
}
impl<S, Fut, Sp> ReactiveBorrow for Rc<MapAsync<S, Fut, Sp>>
where
    S: Reactive<Item = Fut>,
    Fut: Future + 'static,
    Sp: LocalSpawn,
{
    type Item = Poll<Fut::Output>;
    fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        let mut s = self.state.borrow();
        if s.handle.is_none() {
            drop(s);
            self.ready(ctx.scope());
            s = self.state.borrow();
        }
        ctx.bind(self.clone());
        Ref::map(s, |o| &o.value)
    }
}

impl<S, Fut, Sp> DynReBorrowSource for MapAsync<S, Fut, Sp>
where
    S: Reactive<Item = Fut>,
    Fut: Future + 'static,
    Sp: LocalSpawn,
{
    type Item = Poll<Fut::Output>;

    fn dyn_borrow(
        &self,
        rc_self: &Rc<dyn DynReBorrowSource<Item = Self::Item>>,
        ctx: &BindContext,
    ) -> Ref<Self::Item> {
        let rc_self = Self::downcast(rc_self);
        let mut s = self.state.borrow();
        if s.handle.is_none() {
            drop(s);
            rc_self.ready(ctx.scope());
            s = self.state.borrow();
        }
        ctx.bind(rc_self);
        Ref::map(s, |o| &o.value)
    }
    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}

impl<S, Fut, Sp> BindSource for MapAsync<S, Fut, Sp>
where
    S: Reactive<Item = Fut>,
    Fut: Future + 'static,
    Sp: LocalSpawn,
{
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
    fn detach_sink(&self, idx: usize) {
        self.sinks.detach(idx);
        if self.sinks.is_empty() {
            let mut s = self.state.borrow_mut();
            s.handle = None;
            s.value = Poll::Pending;
            s.bindings.clear();
        }
    }
}

impl<S, Fut, Sp> BindSink for MapAsync<S, Fut, Sp>
where
    S: Reactive<Item = Fut>,
    Fut: Future + 'static,
    Sp: LocalSpawn,
{
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        let mut s = self.state.borrow_mut();
        if s.handle.take().is_some() {
            if let Poll::Ready(_) = &s.value {
                s.value = Poll::Pending;
                drop(s);
                self.sinks.notify(ctx);
            } else {
                drop(s);
                ctx.spawn(self);
            }
        }
    }
}
impl<S, Fut, Sp> BindTask for MapAsync<S, Fut, Sp>
where
    S: Reactive<Item = Fut>,
    Fut: Future + 'static,
    Sp: LocalSpawn,
{
    fn run(self: Rc<Self>, scope: &BindContextScope) {
        self.ready(scope);
    }
}
