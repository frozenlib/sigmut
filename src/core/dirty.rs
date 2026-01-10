use std::{
    cmp::max,
    ops::{BitOr, BitOrAssign},
};

use crate::core::NotifyLevel;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub enum Dirty {
    Clean,
    MaybeDirty,
    Dirty,
}
impl Dirty {
    pub fn from_is_dirty(is_dirty: bool) -> Self {
        if is_dirty { Dirty::Dirty } else { Dirty::Clean }
    }
    pub fn is_dirty(self) -> bool {
        self == Dirty::Dirty
    }
    pub fn is_maybe_dirty(self) -> bool {
        self == Dirty::MaybeDirty
    }
    pub fn is_clean(self) -> bool {
        self == Dirty::Clean
    }
    /// Return true if the dependants need to be notified when the dirty state is changed from the current value to `Dirty` or `MaybeDirty`.
    ///
    /// Equivalent to [`is_clean`](Self::is_clean).
    ///
    /// When changing from `MaybeDirty` to `Dirty`,
    /// notification is not necessary because the update is scheduled by the previous `MaybeDirty` notification.
    pub fn needs_notify(self) -> bool {
        self.is_clean()
    }

    pub fn apply_notify(&mut self, level: NotifyLevel) {
        *self = max(*self, level.into());
    }
}

impl BitOr for Dirty {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        max(self, rhs)
    }
}
impl BitOrAssign for Dirty {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs;
    }
}
