use super::*;
use pretty_assertions::assert_eq;
use std::{collections::HashSet, panic::catch_unwind};

#[test]
fn static_identities_are_stable_unique_and_not_limited_to_i8() {
    static PHASES: [ReactionPhase; 512] = [const { ReactionPhase::new() }; 512];
    let ids: Vec<_> = PHASES.iter().map(ReactionPhase::id).collect();
    assert_eq!(ids.iter().copied().collect::<HashSet<_>>().len(), 512);
    assert_eq!(
        ids,
        PHASES.iter().map(ReactionPhase::id).collect::<Vec<_>>()
    );
    assert!(!ids.contains(&ActionPhase::DEFAULT.id()));
}

#[test]
fn identity_exhaustion_panics_without_wrapping() {
    let counter = AtomicUsize::new(usize::MAX - 1);
    assert_eq!(allocate_id(&counter), usize::MAX - 1);
    assert!(catch_unwind(|| allocate_id(&counter)).is_err());
    assert_eq!(counter.load(Ordering::Relaxed), usize::MAX);
}

#[test]
fn lookup_is_dense_in_configured_order_and_never_extends_on_lookup() {
    let order = PhaseOrder::new([9, 1, 4]);
    assert_eq!(
        (
            order.index(9),
            order.index(1),
            order.index(4),
            order.index(2),
            order.index(usize::MAX)
        ),
        (Some(0), Some(1), Some(2), None, None)
    );
    assert_eq!((order.ids, order.to_index.len()), (vec![9, 1, 4], 10));
}
