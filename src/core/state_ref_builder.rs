use std::cell::Ref;

use crate::{SignalContext, StateRef};

pub struct StateRefBuilder<'a, 'b, 'c, T: ?Sized> {
    r: StateRef<'a, T>,
    sc: &'c mut SignalContext<'b>,
}

impl<'a, 'b: 'a, 'c, T: ?Sized> StateRefBuilder<'a, 'b, 'c, T> {
    pub fn new(r: StateRef<'a, T>, sc: &'c mut SignalContext<'b>) -> Self {
        StateRefBuilder { r, sc }
    }

    pub fn from_value(value: T, sc: &'c mut SignalContext<'b>) -> StateRefBuilder<'a, 'b, 'c, T>
    where
        T: Sized + 'static,
    {
        StateRefBuilder {
            r: StateRef::from_value(value, sc),
            sc,
        }
    }
    pub fn from_value_non_static(
        value: T,
        sc: &'c mut SignalContext<'b>,
    ) -> StateRefBuilder<'a, 'b, 'c, T>
    where
        T: Sized,
    {
        StateRefBuilder {
            r: StateRef::from_value_non_static(value, sc),
            sc,
        }
    }
    pub fn from_ref(value: &'a T, sc: &'c mut SignalContext<'b>) -> StateRefBuilder<'a, 'b, 'c, T> {
        StateRefBuilder {
            r: StateRef::from(value),
            sc,
        }
    }
    pub fn from_ref_cell(
        value: Ref<'a, T>,
        sc: &'c mut SignalContext<'b>,
    ) -> StateRefBuilder<'a, 'b, 'c, T> {
        StateRefBuilder {
            r: StateRef::from(value),
            sc,
        }
    }

    pub fn map<U: ?Sized>(
        self,
        f: impl for<'a0> FnOnce(&'a0 T) -> &'a0 U,
    ) -> StateRefBuilder<'a, 'b, 'c, U> {
        StateRefBuilder {
            r: StateRef::map(self.r, f, self.sc),
            sc: self.sc,
        }
    }

    pub fn map_ref<U: ?Sized>(
        self,
        f: impl for<'a0, 'b0> FnOnce(&'a0 T, &mut SignalContext<'b0>, &'a0 &'b0 ()) -> StateRef<'a0, U>,
    ) -> StateRefBuilder<'a, 'b, 'c, U> {
        StateRefBuilder {
            r: StateRef::map_ref(self.r, f, self.sc),
            sc: self.sc,
        }
    }

    pub fn build(self) -> StateRef<'a, T> {
        self.r
    }
}
