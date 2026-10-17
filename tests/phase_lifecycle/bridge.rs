use super::*;
use pretty_assertions::assert_eq;

// A consumer-owned persistent bridge. This fixture intentionally uses only the
// public API; sigmut has no built-in State mirror that needs this protocol.
struct Bridge {
    desired: Cell<i32>,
    revision: Cell<usize>,
    applied: Cell<usize>,
    queued: Cell<bool>,
    target: State<i32>,
}
impl Bridge {
    fn schedule(self: &Rc<Self>) {
        if !self.queued.replace(true) {
            Reaction::from_weak_fn(Rc::downgrade(self), |owner, _| {
                let ticket = Ticket {
                    owner: Rc::downgrade(&owner),
                    applied: false,
                };
                Action::new(move |ac| {
                    let mut ticket = ticket;
                    if let Some(owner) = ticket.owner.upgrade() {
                        let revision = owner.revision.get();
                        owner.target.set(owner.desired.get(), ac);
                        owner.applied.set(revision);
                        owner.queued.set(false);
                        ticket.applied = true;
                    }
                })
                .schedule();
            })
            .schedule_in(&A);
        }
    }
}
struct Ticket {
    owner: Weak<Bridge>,
    applied: bool,
}
impl Drop for Ticket {
    fn drop(&mut self) {
        if !self.applied
            && let Some(owner) = self.owner.upgrade()
        {
            owner.queued.set(false);
            if owner.applied.get() != owner.revision.get() {
                owner.schedule();
            }
        }
    }
}

#[rstest::rstest]
fn unacknowledged_bridge_recovers_latest_revision(#[values(false, true)] drop_owner: bool) {
    let mut first = runtime(&A);
    let source = State::new(1);
    let target = State::new(0);
    let bridge = Rc::new(Bridge {
        desired: Cell::new(0),
        revision: Cell::new(0),
        applied: Cell::new(0),
        queued: Cell::new(false),
        target: target.clone(),
    });
    let subscription = effect_in(&A, {
        let source = source.clone();
        let owner = Rc::downgrade(&bridge);
        move |sc| {
            let value = source.get(sc);
            if let Some(owner) = owner.upgrade() {
                owner.desired.set(value);
                owner.revision.set(owner.revision.get() + 1);
                owner.schedule();
            }
        }
    });
    first.dispatch_all_reactions();
    first.dispatch_all_reactions();
    assert_eq!(target.get(&mut first.sc()), 0);
    first.begin_shutdown(); // Ticket restores just this bridge's Reaction.
    first.flush();
    assert_eq!((bridge.revision.get(), bridge.applied.get()), (1, 0));
    drop(first);
    let mut second = runtime(&B);
    source.set(2, second.ac());
    second.flush();
    assert!(!ready(&mut second));
    drop(second);
    let bridge = if drop_owner {
        drop(bridge);
        None
    } else {
        Some(bridge)
    };
    let mut third = runtime(&A);
    third.flush();
    assert_eq!(target.get(&mut third.sc()), if drop_owner { 0 } else { 2 });
    if let Some(bridge) = bridge {
        assert_eq!((bridge.revision.get(), bridge.applied.get()), (2, 2));
    }
    drop(subscription);
    third.flush();
}

#[test]
fn acknowledged_external_effect_is_not_replayed_by_new_runtime() {
    let calls = Rc::new(Cell::new(0));
    let subscription = effect_in(&A, {
        let calls = calls.clone();
        move |_| calls.set(calls.get() + 1)
    });
    let mut first = runtime(&A);
    first.flush();
    drop(first);
    let mut second = runtime(&A);
    second.flush();
    assert_eq!(calls.get(), 1);
    drop(subscription);
    second.flush();
}
