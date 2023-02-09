use super::RcObservable;
use crate::core::{
    dependency_node::{Compute, DependencyNode, DependencyNodeSettings},
    dependency_token::DependencyToken,
    AsyncObsContext, AsyncObsContextSource, ComputeContext, DependencyWaker, ObsContext,
};
use futures::{Future, Stream};
use std::{
    marker::PhantomData,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

const PARAM: usize = 0;

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
    waker: DependencyWaker,
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
                waker: DependencyWaker::new(this.clone(), PARAM),
            },
            DependencyNodeSettings {
                is_flush: false,
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
    fn compute(&mut self, cc: &mut ComputeContext) -> bool {
        if self.deps.is_up_to_date(cc.oc()) {
            cc.watch_previous_dependencies();
        } else {
            self.deps.update(
                |cc| {
                    self.fut.set(None);
                    let async_oc = self.async_oc_source.context();
                    let value = self.async_oc_source.set(cc.oc(), || (self.f)(async_oc));
                    self.fut.set(Some(value));
                },
                cc.oc(),
            )
        }
        let mut is_modified = false;
        if let Some(f) = self.fut.as_mut().as_pin_mut() {
            let waker = self.waker.as_waker();
            let value = self
                .async_oc_source
                .set(cc.oc(), || f.poll(&mut Context::from_waker(&waker)));
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
    waker: DependencyWaker,
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
                waker: DependencyWaker::new(this.clone(), PARAM),
            },
            DependencyNodeSettings {
                is_flush: false,
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
    fn compute(&mut self, cc: &mut ComputeContext) -> bool {
        if self.deps.is_up_to_date(cc.oc()) {
            cc.watch_previous_dependencies();
        } else {
            self.deps.update(
                |cc| {
                    self.stream.set(None);
                    self.stream.set(Some((self.f)(cc.oc())));
                },
                cc.oc(),
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
}
pub(crate) struct FnStreamScanOps<St, Input, Output: ?Sized, Compute, Get> {
    compute: Compute,
    get: Get,
    _phantom_compute: PhantomData<fn(&mut St, Input)>,
    _phantom_get: PhantomData<fn(&St) -> &Output>,
}

impl<St, Input, Output, Compute, Get> FnStreamScanOps<St, Input, Output, Compute, Get>
where
    Output: ?Sized,
    Compute: Fn(&mut St, Option<Input>) -> bool,
    Get: Fn(&St) -> &Output,
{
    pub fn new(compute: Compute, get: Get) -> Self {
        Self {
            compute,
            get,
            _phantom_compute: PhantomData,
            _phantom_get: PhantomData,
        }
    }
}

impl<St, Input, Output, Compute, Get> StreamScanOps
    for FnStreamScanOps<St, Input, Output, Compute, Get>
where
    Output: ?Sized,
    Compute: Fn(&mut St, Option<Input>) -> bool,
    Get: Fn(&St) -> &Output,
{
    type St = St;
    type Input = Input;
    type Output = Output;
    fn compute(&self, state: &mut Self::St, item: Option<Self::Input>) -> bool {
        (self.compute)(state, item)
    }
    fn get<'a>(&self, state: &'a Self::St) -> &'a Self::Output {
        (self.get)(state)
    }
}

pub(crate) struct FromStreamScan<S, Ops: StreamScanOps> {
    stream: Option<Pin<Box<S>>>,
    state: Ops::St,
    ops: Ops,
    waker: DependencyWaker,
}

impl<S, Ops> FromStreamScan<S, Ops>
where
    S: Stream<Item = Ops::Input> + 'static,
    Ops: StreamScanOps + 'static,
{
    pub(crate) fn new(initial_state: Ops::St, s: S, ops: Ops) -> Rc<DependencyNode<Self>> {
        DependencyNode::new_cyclic(
            |this| Self {
                stream: Some(Box::pin(s)),
                state: initial_state,
                ops,
                waker: DependencyWaker::new(this.clone(), PARAM),
            },
            DependencyNodeSettings {
                is_flush: false,
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
    fn compute(&mut self, _cc: &mut ComputeContext) -> bool {
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
