use std::{
    any::Any,
    cell::{RefCell, RefMut},
    rc::Rc,
};

use derive_ex::derive_ex;
use serde::{Deserialize, Serialize};

use crate::{
    ActionContext, Signal, SignalContext, StateRef,
    core::{
        BindKey, BindSink, BindSource, NotifyContext, NotifyLevel, SinkBindings, Slot,
        UpdateContext, schedule_notify,
    },
    signal::{SignalNode, ToSignal},
};

#[cfg(test)]
mod tests;

/// Similar to `Rc<RefCell<T>>`, but with added functionality to observe changes.
#[derive(Default)]
#[derive_ex(Clone, bound())]
pub struct State<T: 'static>(Rc<StateNode<T>>);

impl<T: 'static> State<T> {
    /// Create a new `State` with the given initial value.
    pub fn new(value: T) -> Self {
        Self(Rc::new(StateNode {
            sinks: RefCell::new(SinkBindings::new()),
            value: RefCell::new(value),
        }))
    }

    /// Obtains a reference to the current value and adds a dependency on this `State` to the specified `SignalContext`.
    pub fn borrow<'a, 's: 'a>(&'a self, sc: &mut SignalContext<'s>) -> StateRef<'a, T> {
        self.0.bind(sc);
        self.0.value.borrow().into()
    }

    /// Gets the current value and adds a dependency on this `State` to the specified `SignalContext`.
    pub fn get(&self, sc: &mut SignalContext) -> T
    where
        T: Clone,
    {
        self.borrow(sc).clone()
    }

    /// Mutably borrows the state.
    ///
    /// This method can only borrow one `State` at a time.
    /// To borrow more than one State at a time, use [`borrow_mut_loose`](Self::borrow_mut_loose).
    ///
    /// When the deref_mut of the return value is called and the borrowing ends, notifications are sent to the dependencies.
    pub fn borrow_mut<'a>(&'a self, ac: &'a mut ActionContext) -> StateRefMut<'a, T> {
        StateRefMut::new(self, Some(ac.nc()))
    }

    /// Mutably borrows the state, disregarding static lifetimes.
    ///
    /// This method can be used to borrow multiple states simultaneously.
    /// Panic if you try to borrow or reference the same state while borrowing.
    pub fn borrow_mut_loose(&self, #[allow(unused)] ac: &ActionContext) -> StateRefMut<'_, T> {
        StateRefMut::new(self, None)
    }

    /// Mutably borrows the state and notify only if the value has changed.
    ///
    /// When borrowing ends and there has been a change in state, notifications are sent to the dependencies.
    ///
    /// This method can only borrow one `State` at a time.
    /// To borrow more than one State at a time, use [`borrow_mut_dedup_loose`](Self::borrow_mut_dedup_loose).
    pub fn borrow_mut_dedup<'a>(&'a self, ac: &'a mut ActionContext) -> StateRefMut<'a, T>
    where
        T: PartialEq + Clone,
    {
        StateRefMut::dedup(self.borrow_mut(ac))
    }

    /// Mutably borrows the state and notify only if the value has changed, disregarding static lifetimes.
    ///
    /// When borrowing ends and there has been a change in state, notifications are sent to the dependencies.
    ///
    /// This method can be used to borrow multiple states simultaneously.
    /// Panic if you try to borrow or reference the same state while borrowing.
    pub fn borrow_mut_dedup_loose(&self, ac: &ActionContext) -> StateRefMut<'_, T>
    where
        T: PartialEq + Clone,
    {
        StateRefMut::dedup(self.borrow_mut_loose(ac))
    }

    /// Sets the value of the state and notifies the dependencies.
    pub fn set(&self, value: T, ac: &mut ActionContext) {
        *self.0.value.borrow_mut() = value;
        self.0.notify_raw(ac.nc());
    }

    /// Sets the value of the state and notifies the dependencies only if the current state is different from the specified value.
    pub fn set_dedup(&self, value: T, ac: &mut ActionContext)
    where
        T: PartialEq,
    {
        let mut this_value = self.0.value.borrow_mut();
        if *this_value != value {
            *this_value = value;
            self.0.notify_raw(ac.nc());
        }
    }

    /// Returns a `Signal` representing this state.
    pub fn to_signal(&self) -> Signal<T> {
        Signal::from_node(self.0.clone())
    }
}
impl<T: std::fmt::Debug> std::fmt::Debug for State<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0.value.try_borrow() {
            Ok(value) => std::fmt::Debug::fmt(&*value, f),
            Err(_) => write!(f, "<borrowed>"),
        }
    }
}
impl<T> Serialize for State<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        match self.0.value.try_borrow() {
            Ok(value) => T::serialize(&*value, serializer),
            Err(_) => Err(serde::ser::Error::custom("borrowed")),
        }
    }
}
impl<'de, T> Deserialize<'de> for State<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<State<T>, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        T::deserialize(deserializer).map(|value| State::new(value))
    }
}
impl<T> ToSignal for State<T> {
    type Value = T;
    fn to_signal(&self) -> Signal<Self::Value> {
        self.to_signal()
    }
}

#[derive(Default)]
struct StateNode<T: 'static> {
    sinks: RefCell<SinkBindings>,
    value: RefCell<T>,
}
impl<T: 'static> StateNode<T> {
    fn bind(self: &Rc<Self>, sc: &mut SignalContext) {
        self.sinks.borrow_mut().bind(self.clone(), Slot(0), sc);
    }
    fn notify_raw(&self, nc: &mut NotifyContext) {
        self.sinks.borrow_mut().notify(NotifyLevel::Dirty, nc)
    }
    fn schedule_notify(self: &Rc<Self>, nc: &mut Option<&mut NotifyContext>) {
        if let Some(nc) = nc {
            self.notify_raw(nc);
        } else {
            let node = Rc::downgrade(self);
            schedule_notify(node, Slot(0))
        }
    }
}

impl<T: 'static> BindSource for StateNode<T> {
    fn check(self: Rc<Self>, _slot: Slot, key: BindKey, uc: &mut UpdateContext) -> bool {
        self.sinks.borrow().is_dirty(key, uc)
    }

    fn unbind(self: Rc<Self>, _slot: Slot, key: BindKey, uc: &mut UpdateContext) {
        self.sinks.borrow_mut().unbind(key, uc);
    }

    fn rebind(self: Rc<Self>, slot: Slot, key: BindKey, sc: &mut SignalContext) {
        self.sinks.borrow_mut().rebind(self.clone(), slot, key, sc);
    }
}
impl<T: 'static> BindSink for StateNode<T> {
    fn notify(self: Rc<Self>, _slot: Slot, _notify_kind: NotifyLevel, nc: &mut NotifyContext) {
        self.notify_raw(nc);
    }
}

impl<T: 'static> SignalNode for StateNode<T> {
    type Value = T;
    fn borrow<'a, 's: 'a>(
        &'a self,
        rc_self: Rc<dyn Any>,
        sc: &mut SignalContext<'s>,
    ) -> StateRef<'a, Self::Value> {
        rc_self.downcast::<Self>().unwrap().bind(sc);
        self.value.borrow().into()
    }

    fn fmt_debug(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result
    where
        Self::Value: std::fmt::Debug,
    {
        match self.value.try_borrow() {
            Ok(value) => value.fmt(f),
            Err(_) => write!(f, "<borrowed>"),
        }
    }
}

enum StateRefMutDirty<T> {
    Unused,
    DedupUnused {
        clone: fn(&T) -> T,
        ne: fn(&T, &T) -> bool,
    },
    DedupUsed {
        old: T,
        ne: fn(&T, &T) -> bool,
    },
    Dirty,
}
impl<T> StateRefMutDirty<T> {
    fn set_dedup(&mut self)
    where
        T: Clone + PartialEq,
    {
        if matches!(self, StateRefMutDirty::Unused) {
            *self = Self::DedupUnused {
                clone: T::clone,
                ne: T::ne,
            };
        }
    }
    fn set_used(&mut self, value: &T) {
        match *self {
            StateRefMutDirty::Unused => *self = StateRefMutDirty::Dirty,
            StateRefMutDirty::DedupUnused { clone, ne, .. } => {
                *self = StateRefMutDirty::DedupUsed {
                    old: clone(value),
                    ne,
                }
            }
            StateRefMutDirty::DedupUsed { .. } | StateRefMutDirty::Dirty => {}
        }
    }
    fn check_dirty(&self, value: &T) -> bool {
        match self {
            StateRefMutDirty::Unused | StateRefMutDirty::DedupUnused { .. } => false,
            StateRefMutDirty::DedupUsed { ne, old } => (ne)(old, value),
            StateRefMutDirty::Dirty => true,
        }
    }
}

pub struct StateRefMut<'a, T: 'static> {
    value: RefMut<'a, T>,
    dirty: StateRefMutDirty<T>,
    node: &'a Rc<StateNode<T>>,
    nc: Option<&'a mut NotifyContext>,
}
impl<'a, T: 'static> StateRefMut<'a, T> {
    fn new(st: &'a State<T>, nc: Option<&'a mut NotifyContext>) -> Self {
        Self {
            value: st.0.value.borrow_mut(),
            dirty: StateRefMutDirty::Unused,
            node: &st.0,
            nc,
        }
    }
    pub fn dedup(mut this: Self) -> Self
    where
        T: PartialEq + Clone,
    {
        this.dirty.set_dedup();
        this
    }

    fn is_dirty(&self) -> bool {
        self.dirty.check_dirty(&self.value)
    }
}
impl<T> std::ops::Deref for StateRefMut<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
impl<T> std::ops::DerefMut for StateRefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.dirty.set_used(&self.value);
        &mut self.value
    }
}
impl<T> Drop for StateRefMut<'_, T> {
    fn drop(&mut self) {
        if self.is_dirty() {
            self.node.schedule_notify(&mut self.nc);
        }
    }
}
