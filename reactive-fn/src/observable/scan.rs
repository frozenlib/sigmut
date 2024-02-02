use super::{ObservableBuilder, RcObservable, Subscription};
use crate::{
    core::{ComputeContext, ObsContext, UpdateContext},
    helpers::dependency_node::{Compute, DependencyNode, DependencyNodeSettings},
    Obs,
};
use std::{marker::PhantomData, rc::Rc};

trait DynFold {
    type State;
    fn stop(self: Rc<Self>, uc: Option<&mut UpdateContext>) -> Self::State;
    fn into_subscription(self: Rc<Self>) -> Subscription;
}

pub(crate) trait IsModified: Copy + 'static {
    const IS_MODIFY_ALWAYS: bool;
    fn is_modified(self) -> bool;
}
impl IsModified for () {
    const IS_MODIFY_ALWAYS: bool = true;
    fn is_modified(self) -> bool {
        true
    }
}
impl IsModified for bool {
    const IS_MODIFY_ALWAYS: bool = false;
    fn is_modified(self) -> bool {
        self
    }
}
pub(crate) trait ScanOps {
    type St;
    type Value: ?Sized;
    type ComputeRet: IsModified;
    fn compute(&self, state: &mut Self::St, oc: &mut ObsContext) -> Self::ComputeRet;
    fn discard(&self, state: &mut Self::St) -> bool;
    fn to_value<'a>(&self, state: &'a Self::St) -> &'a Self::Value;

    fn map<U, F>(self, f: F) -> MapScanOps<Self, F>
    where
        Self: Sized,
        U: ?Sized,
        F: Fn(&Self::Value) -> &U,
    {
        MapScanOps { ops: self, f }
    }
}

pub(crate) struct FnScanOps<St, Compute, ComputeRet, Discard>
where
    St: 'static,
    Compute: Fn(&mut St, &mut ObsContext) -> ComputeRet + 'static,
    ComputeRet: IsModified,
    Discard: Fn(&mut St) -> bool + 'static,
{
    compute: Compute,
    discard: Discard,
    _phantom: std::marker::PhantomData<fn(&mut St)>,
}

impl<St, Compute, ComputeRet, Discard> FnScanOps<St, Compute, ComputeRet, Discard>
where
    St: 'static,
    Compute: Fn(&mut St, &mut ObsContext) -> ComputeRet + 'static,
    ComputeRet: IsModified,
    Discard: Fn(&mut St) -> bool + 'static,
{
    pub fn new(compute: Compute, discard: Discard) -> Self {
        Self {
            compute,
            discard,
            _phantom: PhantomData,
        }
    }
}
impl<St, Compute, ComputeRet, Discard> ScanOps for FnScanOps<St, Compute, ComputeRet, Discard>
where
    St: 'static,
    Compute: Fn(&mut St, &mut ObsContext) -> ComputeRet + 'static,
    ComputeRet: IsModified,
    Discard: Fn(&mut St) -> bool + 'static,
{
    type St = St;
    type Value = St;
    type ComputeRet = ComputeRet;
    fn compute(&self, state: &mut St, oc: &mut ObsContext) -> Self::ComputeRet {
        (self.compute)(state, oc)
    }
    fn discard(&self, state: &mut St) -> bool {
        (self.discard)(state)
    }
    fn to_value<'a>(&self, state: &'a St) -> &'a St {
        state
    }
}

pub(crate) struct AssignOps<F>(pub F);
impl<F, T> ScanOps for AssignOps<F>
where
    F: Fn(&mut ObsContext) -> T,
{
    type St = Option<T>;
    type Value = T;
    type ComputeRet = ();

    fn compute(&self, state: &mut Self::St, oc: &mut ObsContext) -> Self::ComputeRet {
        *state = Some((self.0)(oc))
    }

    fn discard(&self, state: &mut Self::St) -> bool {
        *state = None;
        true
    }
    fn to_value<'a>(&self, state: &'a Self::St) -> &'a Self::Value {
        state.as_ref().unwrap()
    }
}

pub(crate) struct DedupAssignOps<F>(pub F);
impl<F, T> ScanOps for DedupAssignOps<F>
where
    F: Fn(&mut ObsContext) -> T,
    T: PartialEq,
{
    type St = Option<T>;
    type Value = T;
    type ComputeRet = bool;

    fn compute(&self, state: &mut Self::St, oc: &mut ObsContext) -> Self::ComputeRet {
        let new_value = (self.0)(oc);
        if let Some(old_value) = state {
            if old_value == &new_value {
                return false;
            }
        }
        *state = Some(new_value);
        true
    }

    fn discard(&self, state: &mut Self::St) -> bool {
        *state = None;
        true
    }
    fn to_value<'a>(&self, state: &'a Self::St) -> &'a Self::Value {
        state.as_ref().unwrap()
    }
}

pub(crate) struct MapScanOps<Ops, F> {
    ops: Ops,
    f: F,
}
impl<Ops, F, T> ScanOps for MapScanOps<Ops, F>
where
    Ops: ScanOps + 'static,
    F: Fn(&Ops::Value) -> &T + 'static,
    T: ?Sized + 'static,
{
    type St = Ops::St;
    type Value = T;
    type ComputeRet = Ops::ComputeRet;
    fn compute(&self, state: &mut Self::St, oc: &mut ObsContext) -> Self::ComputeRet {
        self.ops.compute(state, oc)
    }
    fn discard(&self, state: &mut Self::St) -> bool {
        self.ops.discard(state)
    }
    fn to_value<'a>(&self, state: &'a Self::St) -> &'a Self::Value {
        (self.f)(self.ops.to_value(state))
    }
}

pub(crate) struct ScanBuilder<Ops: ScanOps + 'static> {
    state: Ops::St,
    ops: Ops,
    is_hot: bool,
}
impl<Ops: ScanOps + 'static> ScanBuilder<Ops> {
    pub(crate) fn new(state: Ops::St, ops: Ops, is_hot: bool) -> Self {
        Self { state, ops, is_hot }
    }
}
impl<Ops> ObservableBuilder for ScanBuilder<Ops>
where
    Ops: ScanOps + 'static,
{
    type Item = Ops::Value;

    fn build_observable(self) -> Rc<DependencyNode<RawScan<Ops>>> {
        RawScan::new(self.state, self.ops, self.is_hot)
    }

    fn build_obs(self) -> crate::Obs<Self::Item> {
        Obs::from_rc_rc(self.build_observable())
    }

    fn map<F, U>(self, f: F) -> impl ObservableBuilder<Item = U>
    where
        F: Fn(&Self::Item) -> &U + 'static,
        U: ?Sized + 'static,
    {
        ScanBuilder {
            state: self.state,
            ops: self.ops.map(f),
            is_hot: self.is_hot,
        }
    }
}

pub(crate) struct RawScan<Ops: ScanOps + 'static> {
    state: Option<Ops::St>,
    ops: Ops,
}

impl<Ops: ScanOps + 'static> RawScan<Ops> {
    fn new(state: Ops::St, ops: Ops, is_hot: bool) -> Rc<DependencyNode<Self>> {
        DependencyNode::new(
            Self {
                state: Some(state),
                ops,
            },
            DependencyNodeSettings {
                is_hasty: false,
                is_hot,
                is_modify_always: Ops::ComputeRet::IS_MODIFY_ALWAYS,
            },
        )
    }
}

impl<Ops: ScanOps + 'static> DynFold for DependencyNode<RawScan<Ops>> {
    type State = Ops::St;
    fn stop(self: Rc<Self>, uc: Option<&mut UpdateContext>) -> Self::State {
        if let Some(uc) = uc {
            self.watch(&mut uc.oc());
        }
        self.borrow_mut().state.take().unwrap()
    }

    fn into_subscription(self: Rc<Self>) -> Subscription {
        Subscription::from_rc(self)
    }
}
impl<Ops: ScanOps + 'static> RcObservable for DependencyNode<RawScan<Ops>> {
    type Item = Ops::Value;

    fn rc_with<U>(
        self: &Rc<Self>,
        f: impl FnOnce(&Self::Item, &mut ObsContext) -> U,
        oc: &mut ObsContext,
    ) -> U {
        self.watch(oc);
        let b = self.borrow();
        f(b.ops.to_value(b.state.as_ref().unwrap()), oc)
    }
}

impl<Ops: ScanOps + 'static> Compute for RawScan<Ops> {
    fn compute(&mut self, cc: ComputeContext) -> bool {
        if let Some(state) = &mut self.state {
            self.ops.compute(state, cc.oc()).is_modified()
        } else {
            false
        }
    }

    fn discard(&mut self) -> bool {
        if let Some(state) = &mut self.state {
            self.ops.discard(state)
        } else {
            true
        }
    }
}

pub struct Fold<St>(Rc<dyn DynFold<State = St>>);

impl<St: 'static> Fold<St> {
    pub fn new(initial_state: St, op: impl Fn(&mut St, &mut ObsContext) + 'static) -> Self {
        let ops = FnScanOps::new(op, |_| false);
        Self(RawScan::new(initial_state, ops, true))
    }

    pub fn stop(self, uc: &mut UpdateContext) -> St {
        self.0.stop(Some(uc))
    }
    pub fn stop_glitch(self) -> St {
        self.0.stop(None)
    }
}

impl<T> From<Fold<T>> for Subscription {
    fn from(value: Fold<T>) -> Self {
        value.0.into_subscription()
    }
}
