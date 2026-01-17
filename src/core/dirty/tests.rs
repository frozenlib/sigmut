use super::*;
use crate::core::DirtyLevel;

#[test]
fn dirty_state_helpers() {
    let mut d = Dirty::Clean;
    d.apply_notify(DirtyLevel::MaybeDirty);
    let after_maybe = d;
    d.apply_notify(DirtyLevel::Dirty);
    let after_dirty = d;

    assert!(Dirty::from_is_dirty(true).is_dirty());
    assert!(Dirty::from_is_dirty(false).is_clean());
    assert!(Dirty::MaybeDirty.is_maybe_dirty());
    assert!(Dirty::Clean.needs_notify());
    assert_eq!(after_maybe, Dirty::MaybeDirty);
    assert_eq!(after_dirty, Dirty::Dirty);
}

#[test]
fn dirty_bitor() {
    let mut d = Dirty::Clean;
    d |= Dirty::MaybeDirty;
    assert_eq!(Dirty::Clean | Dirty::Dirty, Dirty::Dirty);
    assert_eq!(d, Dirty::MaybeDirty);
}
