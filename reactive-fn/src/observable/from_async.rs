use super::{ObservableBuilder, RcObservable};
use crate::{
    core::{AsyncObsContext, AsyncObsContextSource, ObsContext, RuntimeWaker},
    helpers::{
        dependency_node::{Compute, DependencyNode, DependencyNodeSettings},
        dependency_token::DependencyToken,
    },
    Obs,
};
use futures::{Future, Stream};
use std::{
    marker::PhantomData,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

const SLOT: usize = 0;

pub(crate) struct FromAsync<F, Fut>
where
    F: FnMut(AsyncObsContext) -> Fut,
    Fut: Future,
{
    f: F,
    async_oc_source: AsyncObsContextSource,
    deps: DependencyToken,
    fut: Pin<Box<Option<Fut>>>,
    state: Poll<Fut::Output>,
    waker: RuntimeWaker,
}

impl<F, Fut> FromAsync<F, Fut>
where
    F: FnMut(AsyncObsContext) -> Fut + 'static,
    Fut: Future + 'static,
{
    pub(crate) fn new(f: F, is_hot: bool) -> Rc<DependencyNode<Self>> {
        DependencyNode::new_cyclic(
            |this| FromAsync {
                f,
                deps: DependencyToken::new(),
                async_oc_source: AsyncObsContextSource::new(),
                fut: Box::pin(None),
                state: Poll::Pending,
                waker: RuntimeWaker::from_sink(this.clone(), SLOT),
            },
            DependencyNodeSettings {
                is_hasty: false,
                is_hot,
                is_modify_always: false,
            },
        )
    }
}

impl<F, Fut> Compute for FromAsync<F, Fut>
where
    F: FnMut(AsyncObsContext) -> Fut + 'static,
    Fut: Future + 'static,
{
    fn compute(&mut self, oc: &mut ObsContext) -> bool {
        let is_up_to_date = self.deps.is_up_to_date(oc.uc());
        if !is_up_to_date {
            self.deps.update(
                |oc| {
                    self.fut.set(None);
                    let async_oc = self.async_oc_source.context();
                    let value = self.async_oc_source.call(oc.reset(), || (self.f)(async_oc));
                    self.fut.set(Some(value));
                },
                oc.reset(),
            )
        }
        let mut is_modified = false;
        if let Some(f) = self.fut.as_mut().as_pin_mut() {
            let waker = self.waker.as_waker();
            let value = self
                .async_oc_source
                .call(oc, || f.poll(&mut Context::from_waker(&waker)));
            if value.is_ready() {
                self.fut.set(None);
            }
            is_modified = !(self.state.is_pending() && value.is_pending());
            self.state = value;
        };
        is_modified
    }

    fn discard(&mut self) -> bool {
        self.fut.set(None);
        self.state = Poll::Pending;
        true
    }
}
impl<F, Fut> RcObservable for DependencyNode<FromAsync<F, Fut>>
where
    F: FnMut(AsyncObsContext) -> Fut + 'static,
    Fut: Future + 'static,
{
    type Item = Poll<Fut::Output>;

    fn rc_with<U>(
        self: &Rc<Self>,
        f: impl FnOnce(&Self::Item, &mut ObsContext) -> U,
        oc: &mut ObsContext,
    ) -> U {
        self.watch(oc);
        f(&self.borrow().state, oc)
    }
}

pub(crate) struct FromStreamFn<F, S>
where
    F: Fn(&mut ObsContext) -> S + 'static,
    S: Stream + 'static,
{
    f: F,
    deps: DependencyToken,
    stream: Pin<Box<Option<S>>>,
    state: Poll<S::Item>,
    waker: RuntimeWaker,
}

impl<F, S> FromStreamFn<F, S>
where
    F: Fn(&mut ObsContext) -> S + 'static,
    S: Stream + 'static,
{
    pub(crate) fn new(f: F) -> Rc<DependencyNode<Self>> {
        DependencyNode::new_cyclic(
            |this| Self {
                deps: DependencyToken::new(),
                f,
                stream: Box::pin(None),
                state: Poll::Pending,
                waker: RuntimeWaker::from_sink(this.clone(), SLOT),
            },
            DependencyNodeSettings {
                is_hasty: false,
                is_hot: false,
                is_modify_always: false,
            },
        )
    }
}
impl<F, S> Compute for FromStreamFn<F, S>
where
    F: Fn(&mut ObsContext) -> S + 'static,
    S: Stream + 'static,
{
    fn compute(&mut self, oc: &mut ObsContext) -> bool {
        let is_up_to_date = self.deps.is_up_to_date(oc.uc());
        if !is_up_to_date {
            self.deps.update(
                |oc| {
                    self.stream.set(None);
                    self.stream.set(Some((self.f)(oc.reset())));
                },
                oc.reset(),
            )
        }
        let mut is_modified = false;
        if let Some(s) = self.stream.as_mut().as_pin_mut() {
            let waker = self.waker.as_waker();
            let value = s.poll_next(&mut Context::from_waker(&waker));
            if let Poll::Ready(value) = value {
                if let Some(value) = value {
                    is_modified = !self.state.is_pending();
                    self.state = Poll::Ready(value);
                } else {
                    self.stream.set(None);
                }
            }
        };
        is_modified
    }
}
impl<F, S> RcObservable for DependencyNode<FromStreamFn<F, S>>
where
    F: Fn(&mut ObsContext) -> S + 'static,
    S: Stream + 'static,
{
    type Item = Poll<S::Item>;

    fn rc_with<U>(
        self: &Rc<Self>,
        f: impl FnOnce(&Self::Item, &mut ObsContext) -> U,
        oc: &mut ObsContext,
    ) -> U {
        self.watch(oc);
        f(&self.borrow().state, oc)
    }
}

pub(crate) trait StreamScanOps {
    type St;
    type Input;
    type Output: ?Sized;
    fn compute(&self, state: &mut Self::St, item: Option<Self::Input>) -> bool;
    fn get<'a>(&self, state: &'a Self::St) -> &'a Self::Output;

    fn map<F, T>(self, f: F) -> MapStreamScanOps<Self, F>
    where
        Self: Sized,
        F: Fn(&Self::Output) -> &T + 'static,
        T: ?Sized,
    {
        MapStreamScanOps { ops: self, f }
    }
}
pub(crate) struct FnStreamScanOps<St, Input, Compute> {
    compute: Compute,
    _phantom: PhantomData<fn(&mut St, Input)>,
}

impl<St, Input, Compute> FnStreamScanOps<St, Input, Compute>
where
    Compute: Fn(&mut St, Option<Input>) -> bool,
{
    pub fn new(compute: Compute) -> Self {
        Self {
            compute,
            _phantom: PhantomData,
        }
    }
}

impl<St, Input, Compute> StreamScanOps for FnStreamScanOps<St, Input, Compute>
where
    Compute: Fn(&mut St, Option<Input>) -> bool,
{
    type St = St;
    type Input = Input;
    type Output = St;
    fn compute(&self, state: &mut Self::St, item: Option<Self::Input>) -> bool {
        (self.compute)(state, item)
    }
    fn get<'a>(&self, state: &'a Self::St) -> &'a Self::Output {
        state
    }
}

pub(crate) struct MapStreamScanOps<Ops, F> {
    ops: Ops,
    f: F,
}
impl<Ops, F, T> StreamScanOps for MapStreamScanOps<Ops, F>
where
    Ops: StreamScanOps + 'static,
    F: Fn(&Ops::Output) -> &T,
    T: ?Sized,
{
    type St = Ops::St;
    type Input = Ops::Input;
    type Output = T;
    fn compute(&self, state: &mut Self::St, item: Option<Self::Input>) -> bool {
        self.ops.compute(state, item)
    }
    fn get<'a>(&self, state: &'a Self::St) -> &'a Self::Output {
        (self.f)(self.ops.get(state))
    }
}

pub(crate) struct FromStreamScanBuilder<S, Ops: StreamScanOps> {
    initial_state: Ops::St,
    s: S,
    ops: Ops,
}

impl<S, Ops> FromStreamScanBuilder<S, Ops>
where
    S: Stream<Item = Ops::Input> + 'static,
    Ops: StreamScanOps + 'static,
{
    pub(crate) fn new(initial_state: Ops::St, s: S, ops: Ops) -> Self {
        Self {
            initial_state,
            s,
            ops,
        }
    }
}

impl<S, Ops> ObservableBuilder for FromStreamScanBuilder<S, Ops>
where
    S: Stream<Item = Ops::Input> + 'static,
    Ops: StreamScanOps + 'static,
{
    type Item = Ops::Output;

    fn build_observable(self) -> Rc<DependencyNode<FromStreamScan<S, Ops>>> {
        FromStreamScan::new(self.initial_state, self.s, self.ops)
    }

    fn build_obs(self) -> crate::Obs<Self::Item> {
        Obs::from_rc_rc(self.build_observable())
    }

    fn map<F, U>(self, f: F) -> impl ObservableBuilder<Item = U>
    where
        F: Fn(&Self::Item) -> &U + 'static,
        U: ?Sized + 'static,
    {
        FromStreamScanBuilder {
            initial_state: self.initial_state,
            s: self.s,
            ops: self.ops.map(f),
        }
    }
}

pub(crate) struct FromStreamScan<S, Ops: StreamScanOps> {
    stream: Option<Pin<Box<S>>>,
    state: Ops::St,
    ops: Ops,
    waker: RuntimeWaker,
}

impl<S, Ops> FromStreamScan<S, Ops>
where
    S: Stream<Item = Ops::Input> + 'static,
    Ops: StreamScanOps + 'static,
{
    fn new(initial_state: Ops::St, s: S, ops: Ops) -> Rc<DependencyNode<Self>> {
        DependencyNode::new_cyclic(
            |this| Self {
                stream: Some(Box::pin(s)),
                state: initial_state,
                ops,
                waker: RuntimeWaker::from_sink(this.clone(), SLOT),
            },
            DependencyNodeSettings {
                is_hasty: false,
                is_hot: false,
                is_modify_always: false,
            },
        )
    }
}

impl<S, Ops> Compute for FromStreamScan<S, Ops>
where
    S: Stream<Item = Ops::Input> + 'static,
    Ops: StreamScanOps + 'static,
{
    fn compute(&mut self, _oc: &mut ObsContext) -> bool {
        let mut is_modified = false;
        if let Some(s) = self.stream.as_mut() {
            let waker = self.waker.as_waker();
            let item = s.as_mut().poll_next(&mut Context::from_waker(&waker));
            match item {
                Poll::Ready(item) => {
                    if item.is_none() {
                        self.stream = None;
                    }
                    is_modified = self.ops.compute(&mut self.state, item);
                }
                Poll::Pending => {}
            }
        }
        is_modified
    }
}
impl<S, Ops> RcObservable for DependencyNode<FromStreamScan<S, Ops>>
where
    S: Stream<Item = Ops::Input> + 'static,
    Ops: StreamScanOps + 'static,
{
    type Item = Ops::Output;

    fn rc_with<U>(
        self: &Rc<Self>,
        f: impl FnOnce(&Self::Item, &mut ObsContext) -> U,
        oc: &mut ObsContext,
    ) -> U {
        self.watch(oc);
        let b = self.borrow();
        f(b.ops.get(&b.state), oc)
    }
}
