use std::cell::Ref;

use crate::{SignalContext, StateRef};

/// A builder for creating a [`StateRef`].
pub struct StateRefBuilder<'a, 'b, 'r: 'a, 's, T: ?Sized + 'a> {
    r: StateRef<'a, T>,
    sc: &'b mut SignalContext<'r, 's>,
}

impl<'a, 'b, 'r: 'a, 's, T: ?Sized> StateRefBuilder<'a, 'b, 'r, 's, T> {
    pub fn new(r: StateRef<'a, T>, sc: &'b mut SignalContext<'r, 's>) -> Self {
        StateRefBuilder { r, sc }
    }

    pub fn from_value(
        value: T,
        sc: &'b mut SignalContext<'r, 's>,
    ) -> StateRefBuilder<'a, 'b, 'r, 's, T>
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
        sc: &'b mut SignalContext<'r, 's>,
    ) -> StateRefBuilder<'a, 'b, 'r, 's, T>
    where
        T: Sized,
    {
        StateRefBuilder {
            r: StateRef::from_value_non_static(value, sc),
            sc,
        }
    }
    pub fn from_ref(
        value: &'a T,
        sc: &'b mut SignalContext<'r, 's>,
    ) -> StateRefBuilder<'a, 'b, 'r, 's, T> {
        StateRefBuilder {
            r: StateRef::from(value),
            sc,
        }
    }
    pub fn from_ref_cell(
        value: Ref<'a, T>,
        sc: &'b mut SignalContext<'r, 's>,
    ) -> StateRefBuilder<'a, 'b, 'r, 's, T> {
        StateRefBuilder {
            r: StateRef::from(value),
            sc,
        }
    }

    pub fn map<U: ?Sized>(
        self,
        f: impl for<'a0> FnOnce(&'a0 T) -> &'a0 U,
    ) -> StateRefBuilder<'a, 'b, 'r, 's, U> {
        StateRefBuilder {
            r: StateRef::map(self.r, f, self.sc),
            sc: self.sc,
        }
    }

    pub fn map_ref<U: ?Sized>(
        self,
        f: impl for<'a0, 'r0> FnOnce(
            &'a0 T,
            &mut SignalContext<'r0, '_>,
            &'a0 &'r0 (),
        ) -> StateRef<'a0, U>,
    ) -> StateRefBuilder<'a, 'b, 'r, 's, U> {
        StateRefBuilder {
            r: StateRef::map_ref(self.r, f, self.sc),
            sc: self.sc,
        }
    }

    pub fn build(self) -> StateRef<'a, T> {
        self.r
    }
}

#[cfg(test)]
mod tests;
