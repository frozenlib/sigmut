use std::cell::Ref;

use super::{ObsContext, ObsRef};

pub struct ObsRefBuilder<'a, 'b, 'c, T: ?Sized> {
    r: ObsRef<'a, T>,
    oc: &'c mut ObsContext<'b>,
}

impl<'a, 'b: 'a, 'c, T: ?Sized> ObsRefBuilder<'a, 'b, 'c, T> {
    pub fn new(r: ObsRef<'a, T>, oc: &'c mut ObsContext<'b>) -> Self {
        ObsRefBuilder { r, oc }
    }

    pub fn from_value(value: T, oc: &'c mut ObsContext<'b>) -> ObsRefBuilder<'a, 'b, 'c, T>
    where
        T: Sized + 'static,
    {
        ObsRefBuilder {
            r: ObsRef::from_value(value, oc),
            oc,
        }
    }
    pub fn from_value_non_static(
        value: T,
        oc: &'c mut ObsContext<'b>,
    ) -> ObsRefBuilder<'a, 'b, 'c, T>
    where
        T: Sized,
    {
        ObsRefBuilder {
            r: ObsRef::from_value_non_static(value, oc),
            oc,
        }
    }
    pub fn from_ref(value: &'a T, oc: &'c mut ObsContext<'b>) -> ObsRefBuilder<'a, 'b, 'c, T> {
        ObsRefBuilder {
            r: ObsRef::from(value),
            oc,
        }
    }
    pub fn from_ref_cell(
        value: Ref<'a, T>,
        oc: &'c mut ObsContext<'b>,
    ) -> ObsRefBuilder<'a, 'b, 'c, T> {
        ObsRefBuilder {
            r: ObsRef::from(value),
            oc,
        }
    }

    pub fn map<U: ?Sized>(
        self,
        f: impl for<'a0> FnOnce(&'a0 T) -> &'a0 U,
    ) -> ObsRefBuilder<'a, 'b, 'c, U> {
        ObsRefBuilder {
            r: ObsRef::map(self.r, f, self.oc),
            oc: self.oc,
        }
    }

    pub fn map_ref<U: ?Sized>(
        self,
        f: impl for<'a0, 'b0> FnOnce(&'a0 T, &mut ObsContext<'b0>, &'a0 &'b0 ()) -> ObsRef<'a0, U>,
    ) -> ObsRefBuilder<'a, 'b, 'c, U> {
        ObsRefBuilder {
            r: ObsRef::map_ref(self.r, f, self.oc),
            oc: self.oc,
        }
    }

    pub fn build(self) -> ObsRef<'a, T> {
        self.r
    }
}
