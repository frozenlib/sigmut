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

pub struct MapAsync<Fut, Sp>
where
    Fut: Future + 'static,
    Sp: LocalSpawn,
{
    sp: Sp,
    source: Re<Fut>,
    sinks: BindSinks,
    state: RefCell<MapAsyncState<Fut::Output, Sp::Handle>>,
}
struct MapAsyncState<T, H> {
    value: Poll<T>,
    handle: Option<H>,
    bindings: Bindings,
}

impl<Fut, Sp> MapAsync<Fut, Sp>
where
    Fut: Future + 'static,
    Sp: LocalSpawn,
{
    pub fn new(source: Re<Fut>, sp: Sp) -> Self {
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

    fn ready(self: &Rc<Self>, ctx: &BindContext) {
        let mut s = self.state.borrow_mut();
        let fut = s.bindings.update(ctx, self, |ctx| self.source.get(ctx));
        let this = Rc::downgrade(self);
        s.handle = Some(self.sp.spawn_local(async move {
            let value = fut.await;
            if let Some(this) = Weak::upgrade(&this) {
                let mut s = this.state.borrow_mut();
                s.value = Poll::Ready(value);
                drop(s);
                this.sinks.notify_and_update();
            }
        }));
    }
}

impl<Fut, Sp> DynReBorrowSource for MapAsync<Fut, Sp>
where
    Fut: Future + 'static,
    Sp: LocalSpawn,
{
    type Item = Poll<Fut::Output>;

    fn dyn_borrow(
        &self,
        rc_self: &Rc<dyn DynReBorrowSource<Item = Self::Item>>,
        ctx: &mut BindContext,
    ) -> Ref<Self::Item> {
        let rc_self = Self::downcast(rc_self);
        let mut s = self.state.borrow();
        if s.handle.is_none() {
            drop(s);
            rc_self.ready(ctx);
            s = self.state.borrow();
        }
        ctx.bind(rc_self);
        Ref::map(s, |o| &o.value)
    }
    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}

impl<Fut, Sp> BindSource for MapAsync<Fut, Sp>
where
    Fut: Future + 'static,
    Sp: LocalSpawn,
{
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}

impl<Fut, Sp> BindSink for MapAsync<Fut, Sp>
where
    Fut: Future + 'static,
    Sp: LocalSpawn,
{
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        let mut s = self.state.borrow_mut();
        if s.handle.is_some() {
            s.handle = None;
            if let Poll::Ready(_) = &s.value {
                s.value = Poll::Pending;
                drop(s);
                self.sinks.notify(ctx);
            }
        }
    }
}
