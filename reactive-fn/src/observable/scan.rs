use super::{RcObservable, Subscription};
use crate::core::{
    dependency_node::{Compute, DependencyNode, DependencyNodeSettings},
    ComputeContext, ObsContext,
};
use std::rc::Rc;

trait DynFold {
    type State;
    fn stop(self: Rc<Self>, oc: Option<&mut ObsContext>) -> Self::State;
    fn into_subscription(self: Rc<Self>) -> Subscription;
}

pub(crate) trait IsModified: Copy + 'static {
    const IS_MODIFIY_ALWAYS: bool;
    fn is_modified(self) -> bool;
}
impl IsModified for () {
    const IS_MODIFIY_ALWAYS: bool = true;
    fn is_modified(self) -> bool {
        true
    }
}
impl IsModified for bool {
    const IS_MODIFIY_ALWAYS: bool = false;
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
}

pub(crate) struct FnScanOps<St, Value, Compute, ComputeRet, ToValue, Discard>
where
    St: 'static,
    Value: ?Sized + 'static,
    Compute: Fn(&mut St, &mut ObsContext) -> ComputeRet + 'static,
    ComputeRet: IsModified,
    ToValue: Fn(&St) -> &Value + 'static,
    Discard: Fn(&mut St) -> bool + 'static,
{
    compute: Compute,
    discard: Discard,
    to_value: ToValue,
    _phantom: std::marker::PhantomData<fn(&mut St) -> &Value>,
}

impl<St, Value, Compute, ComputeRet, ToValue, Discard>
    FnScanOps<St, Value, Compute, ComputeRet, ToValue, Discard>
where
    St: 'static,
    Value: ?Sized + 'static,
    Compute: Fn(&mut St, &mut ObsContext) -> ComputeRet + 'static,
    ComputeRet: IsModified,
    ToValue: Fn(&St) -> &Value + 'static,
    Discard: Fn(&mut St) -> bool + 'static,
{
    pub fn new(compute: Compute, discard: Discard, to_value: ToValue) -> Self {
        Self {
            compute,
            discard,
            to_value,
            _phantom: std::marker::PhantomData,
        }
    }
}
impl<St, Value, Compute, ComputeRet, Map, Discard> ScanOps
    for FnScanOps<St, Value, Compute, ComputeRet, Map, Discard>
where
    St: 'static,
    Value: ?Sized + 'static,
    Compute: Fn(&mut St, &mut ObsContext) -> ComputeRet + 'static,
    ComputeRet: IsModified,
    Map: Fn(&St) -> &Value + 'static,
    Discard: Fn(&mut St) -> bool + 'static,
{
    type St = St;
    type Value = Value;
    type ComputeRet = ComputeRet;
    fn compute(&self, state: &mut St, oc: &mut ObsContext) -> Self::ComputeRet {
        (self.compute)(state, oc)
    }
    fn discard(&self, state: &mut St) -> bool {
        (self.discard)(state)
    }
    fn to_value<'a>(&self, state: &'a St) -> &'a Value {
        (self.to_value)(state)
    }
}

pub(crate) struct RawScan<Ops: ScanOps + 'static> {
    state: Option<Ops::St>,
    ops: Ops,
}

impl<Ops: ScanOps + 'static> RawScan<Ops> {
    pub(crate) fn new(state: Ops::St, ops: Ops, is_hot: bool) -> Rc<DependencyNode<Self>> {
        DependencyNode::new(
            Self {
                state: Some(state),
                ops,
            },
            DependencyNodeSettings {
                is_flush: false,
                is_hot,
                is_modify_always: Ops::ComputeRet::IS_MODIFIY_ALWAYS,
            },
        )
    }
}

impl<Ops: ScanOps + 'static> DynFold for DependencyNode<RawScan<Ops>> {
    type State = Ops::St;
    fn stop(self: Rc<Self>, oc: Option<&mut ObsContext>) -> Self::State {
        if let Some(oc) = oc {
            self.watch(&mut oc.nul());
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
    fn compute(&mut self, cc: &mut ComputeContext) -> bool {
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
        let ops = FnScanOps::new(op, |_| false, |st| st);
        Self(RawScan::new(initial_state, ops, true))
    }

    pub fn stop(self, oc: &mut ObsContext) -> St {
        self.0.stop(Some(oc))
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
