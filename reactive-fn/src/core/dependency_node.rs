use crate::{
    core::{BindSink, BindSource, ComputeContext, Computed, Runtime, SinkBindings},
    ActionContext, ObsContext,
};
use std::{
    cell::{BorrowError, BorrowMutError, Ref, RefCell, RefMut},
    rc::{Rc, Weak},
};

use super::{CallDiscard, CallFlush, CallUpdate, LazyTasks, SourceBindings};

const PARAM: usize = 0;

pub trait Compute {
    fn compute(&mut self, cc: &mut ComputeContext) -> bool;
    fn discard(&mut self) -> bool {
        true
    }
}

#[derive(Copy, Clone, Default, Debug, Eq, PartialEq)]
pub struct DependencyNodeSettings {
    /// If this value is true and this node is in use, it is recomputed before other nodes,
    /// and this node does not convey the "value may be out of date" notification to the dependent nodes.
    ///
    /// Therefore, performance is better when the state of the dependent node is updated frequently, and the state of this node is updated only rarely.
    ///
    /// However, if the `is_flush` node depends on the `is_flush` node,
    /// it may perform computation  based on the old state due to incomplete notification of state changes.
    ///
    /// If the computation is based on the old state,
    /// the computation for this node is performed again before the computation for the node with `is_flush` false is performed.
    pub is_flush: bool,

    pub is_hot: bool,

    /// If this value is true, always change the state of this node regardless of the return value of [`Compute::compute`].
    pub is_modify_always: bool,
}

pub struct DependencyNode<T, D = ()> {
    c: ComputeBindings<T>,
    d: RefCell<SinksAndState>,
    s: DependencyNodeSettings,
    pub data: D,
}

impl<T, D> DependencyNode<T, D>
where
    T: Compute + 'static,
    D: Default + 'static,
{
    pub fn new(value: T, settings: DependencyNodeSettings) -> Rc<Self> {
        Self::new_cyclic(|_| value, settings)
    }
    pub fn new_cyclic(
        value: impl FnOnce(&Weak<Self>) -> T,
        settings: DependencyNodeSettings,
    ) -> Rc<Self> {
        let this = Rc::new_cyclic(|this| Self {
            c: ComputeBindings::new(value(this)),
            d: RefCell::new(SinksAndState::new()),
            s: settings,
            data: Default::default(),
        });
        if this.s.is_hot {
            let node = Rc::downgrade(&this);
            LazyTasks::schedule_update(node, PARAM);
        }
        this
    }
}
impl<T, D> DependencyNode<T, D>
where
    T: Compute + 'static,
    D: 'static,
{
    pub fn notify(self: &Rc<Self>, ac: &mut ActionContext) {
        NodeHelper::new(self, ac.rt()).state().notify(true);
    }
    pub fn notify_lazy(self: &Rc<Self>) {
        if let Ok(mut s) = self.d.try_borrow_mut() {
            if s.state.computed.is_may_up_to_date() && !s.state.is_scheduled_notify {
                s.state.is_scheduled_notify = true;
            } else {
                return;
            }
        }
        let node = Rc::downgrade(self);
        Runtime::schedule_notify_lazy(node, PARAM)
    }
    pub fn is_up_to_date(self: &Rc<Self>, oc: &mut ObsContext) -> bool {
        NodeHelper::new(self, oc.rt).state().is_up_to_date().1
    }
    pub fn watch(self: &Rc<Self>, oc: &mut ObsContext) {
        let mut d = self.d.borrow_mut();
        d.sinks.watch(self.clone(), PARAM, oc);
        NodeHelper::new(self, oc.rt).state_with(d).update();
    }

    pub fn borrow(&self) -> Ref<T> {
        Ref::map(self.c.0.borrow(), |d| &d.value)
    }
    pub fn try_borrow(&self) -> Result<Ref<'_, T>, BorrowError> {
        Ok(Ref::map(self.c.0.try_borrow()?, |d| &d.value))
    }
    pub fn borrow_mut(&self) -> RefMut<T> {
        RefMut::map(self.c.0.borrow_mut(), |d| &mut d.value)
    }
    pub fn try_borrow_mut(&self) -> Result<RefMut<'_, T>, BorrowMutError> {
        Ok(RefMut::map(self.c.0.try_borrow_mut()?, |d| &mut d.value))
    }
}

impl<T, D> BindSink for DependencyNode<T, D>
where
    T: Compute + 'static,
    D: 'static,
{
    fn notify(self: Rc<Self>, _param: usize, is_modified: bool, rt: &mut Runtime) {
        let mut h = NodeHelper::new(&self, rt).state();
        h.d.state.is_scheduled_notify = false;
        h.notify(is_modified)
    }
}

impl<T, D> BindSource for DependencyNode<T, D>
where
    T: Compute + 'static,
    D: 'static,
{
    fn flush(self: Rc<Self>, _param: usize, rt: &mut Runtime) -> bool {
        NodeHelper::new(&self, rt).state().flush().1
    }
    fn unbind(self: Rc<Self>, _param: usize, key: usize, rt: &mut Runtime) {
        NodeHelper::new(&self, rt).state().unbind_sink(key);
    }
}
impl<T, D> CallFlush for DependencyNode<T, D>
where
    T: Compute + 'static,
    D: 'static,
{
    fn call_flush(self: Rc<Self>, _param: usize, rt: &mut Runtime) {
        let mut h = NodeHelper::new(&self, rt).state();
        h.d.state.is_scheduled_flush = false;
        h.flush();
    }
}
impl<T, D> CallUpdate for DependencyNode<T, D>
where
    T: Compute + 'static,
    D: 'static,
{
    fn call_update(self: Rc<Self>, _param: usize, rt: &mut Runtime) {
        let mut h = NodeHelper::new(&self, rt).state();
        h.d.state.is_scheduled_update = false;
        h.update();
    }
}
impl<T, D> CallDiscard for DependencyNode<T, D>
where
    T: Compute + 'static,
    D: 'static,
{
    fn call_discard(self: Rc<Self>, _param: usize, rt: &mut Runtime) {
        let mut h = NodeHelper::new(&self, rt).state();
        h.d.state.is_scheduled_discard = false;
        h.discard();
    }
}

struct NodeHelper<'a, T, D>
where
    T: Compute + 'static,
    D: 'static,
{
    node: &'a Rc<DependencyNode<T, D>>,
    rt: &'a mut Runtime,
}

impl<'a, T, D> NodeHelper<'a, T, D>
where
    T: Compute + 'static,
    D: 'static,
{
    fn new(node: &'a Rc<DependencyNode<T, D>>, rt: &'a mut Runtime) -> Self {
        Self { node, rt }
    }

    fn state(self) -> NodeStateHelper<'a, T, D> {
        let d = self.node.d.borrow_mut();
        self.state_with(d)
    }
    fn state_with(self, d: RefMut<'a, SinksAndState>) -> NodeStateHelper<'a, T, D> {
        let s = self.node.s;
        NodeStateHelper { h: self, s, d }
    }
}

struct NodeStateHelper<'a, T, D>
where
    T: Compute + 'static,
    D: 'static,
{
    h: NodeHelper<'a, T, D>,
    s: DependencyNodeSettings,
    d: RefMut<'a, SinksAndState>,
}

impl<'a, T, D> NodeStateHelper<'a, T, D>
where
    T: Compute + 'static,
    D: 'static,
{
    fn notify(&mut self, is_modified: bool) {
        if !self.d.state.computed.modify(is_modified) {
            return;
        }
        if self.s.is_flush && self.is_using() && (!is_modified || !self.s.is_modify_always) {
            self.schedule_flush();
        } else if self.s.is_hot {
            self.schedule_update();
        } else {
            self.try_schedule_discard();
        }
        let is_modified = is_modified && self.s.is_modify_always;
        if is_modified || !self.s.is_flush {
            self.notify_sinks(is_modified);
        }
    }
    fn notify_sinks(&mut self, is_modified: bool) {
        self.d.sinks.notify(is_modified, self.h.rt);
    }
    fn is_using(&mut self) -> bool {
        !self.d.sinks.is_empty()
    }

    #[allow(clippy::wrong_self_convention)]
    fn is_up_to_date(mut self) -> (Self, bool) {
        if self.d.state.computed == Computed::MayBeOutdated {
            self = self.flush_sources().0;
        }
        let is_up_to_date = self.d.state.computed == Computed::UpToDate;
        (self, is_up_to_date)
    }
    fn flush(mut self) -> (Self, bool) {
        let mut is_modified = false;
        if self.is_using() {
            (self, is_modified) = if self.s.is_modify_always {
                self.flush_sources()
            } else {
                self.update()
            };
        }
        if self.d.state.computed != Computed::UpToDate {
            if self.s.is_hot {
                self.schedule_update();
            } else {
                self.try_schedule_discard();
            }
        }
        (self, is_modified)
    }
    fn flush_sources(mut self) -> (Self, bool) {
        let h = self.finish();
        let is_modified = h.node.c.flush(h.rt);
        self = h.state();
        if !is_modified {
            self.d.state.computed = Computed::UpToDate;
        }
        (self, is_modified)
    }
    fn update(self) -> (Self, bool) {
        let (mut this, is_up_to_date) = self.is_up_to_date();
        if is_up_to_date {
            return (this, false);
        }
        let h = this.finish();
        let node = Rc::downgrade(h.node);
        let is_modified = h.node.c.compute(node, PARAM, |st, cc| st.compute(cc), h.rt);
        this = h.state();
        if is_modified && !this.s.is_modify_always {
            this.notify_sinks(true);
        }
        this.d.state.computed = Computed::UpToDate;
        this.try_schedule_discard();
        (this, is_modified)
    }
    fn can_discard(&self) -> bool {
        !self.s.is_hot && self.d.sinks.is_empty() && self.d.state.computed != Computed::None
    }
    fn discard(mut self) {
        if self.can_discard() {
            self.h.node.c.discard(|st| st.discard(), self.h.rt);
            self.d.state.computed = Computed::None;
        }
    }

    fn unbind_sink(&mut self, key: usize) {
        self.d.sinks.unbind(key);
        self.try_schedule_discard();
    }

    fn schedule_flush(&mut self) {
        if !self.d.state.is_scheduled_flush {
            self.d.state.is_scheduled_flush = true;
            self.h.rt.schedule_flush(self.h.node.clone(), PARAM);
        }
    }

    fn schedule_update(&mut self) {
        if !self.d.state.is_scheduled_update {
            self.d.state.is_scheduled_update = true;
            self.h.rt.schedule_update(self.h.node.clone(), PARAM);
        }
    }
    fn try_schedule_discard(&mut self) {
        if !self.d.state.is_scheduled_discard && self.can_discard() {
            self.d.state.is_scheduled_discard = true;
            self.h.rt.schedule_discard(self.h.node.clone(), PARAM);
        }
    }
    fn finish(self) -> NodeHelper<'a, T, D> {
        self.h
    }
}

struct SinksAndState {
    sinks: SinkBindings,
    state: State,
}

impl SinksAndState {
    fn new() -> Self {
        Self {
            sinks: SinkBindings::new(),
            state: State::new(),
        }
    }
}

struct State {
    computed: Computed,
    is_scheduled_notify: bool,
    is_scheduled_update: bool,
    is_scheduled_flush: bool,
    is_scheduled_discard: bool,
}

impl State {
    fn new() -> Self {
        Self {
            computed: Computed::None,
            is_scheduled_notify: false,
            is_scheduled_update: false,
            is_scheduled_flush: false,
            is_scheduled_discard: false,
        }
    }
}

struct ComputeBindings<T>(RefCell<ComputeBindingsData<T>>);

impl<T> ComputeBindings<T> {
    pub fn new(value: T) -> Self {
        Self(RefCell::new(ComputeBindingsData::new(value)))
    }
    pub fn flush(&self, rt: &mut Runtime) -> bool {
        self.0.borrow().bindings.flush(rt)
    }
    pub fn compute<U>(
        &self,
        node: Weak<dyn BindSink>,
        param: usize,
        compute: impl FnOnce(&mut T, &mut ComputeContext) -> U,
        rt: &mut Runtime,
    ) -> U {
        self.0.borrow_mut().compute(node, param, compute, rt)
    }
    fn discard(&self, discard: impl FnOnce(&mut T) -> bool, rt: &mut Runtime) {
        self.0.borrow_mut().discard(discard, rt)
    }
}

struct ComputeBindingsData<T> {
    bindings: SourceBindings,
    value: T,
}

impl<T> ComputeBindingsData<T> {
    fn new(value: T) -> Self {
        Self {
            bindings: SourceBindings::new(),
            value,
        }
    }
    fn compute<U>(
        &mut self,
        node: Weak<dyn BindSink>,
        param: usize,
        compute: impl FnOnce(&mut T, &mut ComputeContext) -> U,
        rt: &mut Runtime,
    ) -> U {
        self.bindings
            .compute(node, param, |cc| compute(&mut self.value, cc), rt)
    }
    fn discard(&mut self, discard: impl FnOnce(&mut T) -> bool, rt: &mut Runtime) {
        if discard(&mut self.value) {
            for s in self.bindings.0.drain(..) {
                s.unbind(rt);
            }
        }
    }
}