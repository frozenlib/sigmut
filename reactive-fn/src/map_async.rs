use super::*;
use std::{
    cell::{Ref, RefCell},
    future::Future,
    rc::{Rc, Weak},
    task::Poll,
};

pub trait LocalSpawn: 'static {
    type Handle;
    fn spawn_local(&self, fut: impl Future<Output = ()> + 'static) -> Self::Handle;
}

pub struct MapAsync<S, Fut, Sp>
where
    S: Observable<Item = Fut>,
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
    S: Observable<Item = Fut>,
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

    fn ready(self: &Rc<Self>, scope: &BindScope) {
        let mut s = self.state.borrow_mut();
        let fut = s.bindings.update(scope, self, |cx| self.source.get(cx));
        let this = Rc::downgrade(self);
        s.handle = Some(self.sp.spawn_local(async move {
            let value = fut.await;
            if let Some(this) = Weak::upgrade(&this) {
                this.state.borrow_mut().value = Poll::Ready(value);
                Runtime::spawn_notify(this);
            }
        }));
    }
    fn borrow<'a>(self: &'a Rc<Self>, cx: &mut BindContext) -> Ref<'a, Poll<Fut::Output>> {
        self.borrow_with(self.clone(), cx)
    }
    fn borrow_with(&self, rc_self: Rc<Self>, cx: &mut BindContext) -> Ref<Poll<Fut::Output>> {
        let mut s = self.state.borrow();
        if s.handle.is_none() {
            drop(s);
            rc_self.ready(cx.scope());
            s = self.state.borrow();
        }
        cx.bind(rc_self);
        Ref::map(s, |o| &o.value)
    }
}
// impl<S, Fut, Sp> ObservableBorrow for Rc<MapAsync<S, Fut, Sp>>
// where
//     S: Observable<Item = Fut>,
//     Fut: Future + 'static,
//     Sp: LocalSpawn,
// {
//     type Item = Poll<Fut::Output>;
//     fn borrow(&self, cx: &mut BindContext) -> Ref<Self::Item> {
//         self.borrow(cx)
//     }
// }

// impl<S, Fut, Sp> DynamicObservableBorrowSource for MapAsync<S, Fut, Sp>
// where
//     S: Observable<Item = Fut>,
//     Fut: Future + 'static,
//     Sp: LocalSpawn,
// {
//     type Item = Poll<Fut::Output>;

//     fn dyn_borrow<'a>(
//         &'a self,
//         rc_self: &Rc<dyn DynamicObservableBorrowSource<Item = Self::Item>>,
//         cx: &mut BindContext,
//     ) -> Ref<Self::Item> {
//         self.borrow_with(Self::downcast(rc_self), cx)
//     }
//     fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any> {
//         self
//     }
//     fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRefSource<Item = Self::Item>> {
//         self
//     }
// }
impl<S, Fut, Sp> DynamicObservableInner for MapAsync<S, Fut, Sp>
where
    S: Observable<Item = Fut>,
    Fut: Future + 'static,
    Sp: LocalSpawn,
{
    type Item = Poll<Fut::Output>;
    fn dyn_with(
        self: Rc<Self>,
        f: &mut dyn FnMut(&Self::Item, &mut BindContext),
        cx: &mut BindContext,
    ) {
        f(&self.borrow(cx), cx)
    }
}

impl<S, Fut, Sp> BindSource for MapAsync<S, Fut, Sp>
where
    S: Observable<Item = Fut>,
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
    S: Observable<Item = Fut>,
    Fut: Future + 'static,
    Sp: LocalSpawn,
{
    fn notify(self: Rc<Self>, scope: &NotifyScope) {
        let mut s = self.state.borrow_mut();
        if s.handle.take().is_some() {
            if s.value.is_ready() {
                s.value = Poll::Pending;
                drop(s);
                self.sinks.notify(scope);
            } else {
                drop(s);
                scope.defer_bind(self);
            }
        }
    }
}
impl<S, Fut, Sp> BindTask for MapAsync<S, Fut, Sp>
where
    S: Observable<Item = Fut>,
    Fut: Future + 'static,
    Sp: LocalSpawn,
{
    fn run(self: Rc<Self>, scope: &BindScope) {
        self.ready(scope);
    }
}
