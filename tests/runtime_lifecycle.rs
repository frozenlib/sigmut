use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use pretty_assertions::assert_eq;
use sigmut::{Action, Reaction, SignalBuilder, State, Subscription, core::Runtime, effect};

#[rstest::rstest]
fn effect_reservation_survives_runtime_drop(#[values(false, true)] initialized: bool) {
    let mut runtime = Runtime::new();
    let state = State::new(1);
    let values = Rc::new(std::cell::RefCell::new(Vec::new()));
    let _subscription = effect({
        let state = state.clone();
        let values = values.clone();
        move |sc| values.borrow_mut().push(state.get(sc))
    });
    if initialized {
        runtime.flush();
        state.set(2, runtime.ac());
    }
    drop(runtime);
    let mut runtime = Runtime::new();
    runtime.flush();
    assert_eq!(
        *values.borrow(),
        if initialized { vec![1, 2] } else { vec![1] }
    );
}

#[test]
fn discard_reservation_survives_runtime_drop() {
    let mut runtime = Runtime::new();
    let discards = Rc::new(Cell::new(0));
    runtime.rc().schedule_discard(Reaction::new({
        let discards = discards.clone();
        move |_| discards.set(discards.get() + 1)
    }));
    drop(runtime);
    let mut runtime = Runtime::new();
    runtime.flush();
    assert_eq!(discards.get(), 1);
}

#[test]
fn explicit_action_registration_requires_runtime() {
    assert!(catch_unwind(|| Action::new(|_| {}).schedule()).is_err());
}

#[rstest::rstest]
#[case::action(false)]
#[case::reaction(true)]
fn shutdown_releases_subscription_owned_by_pending_work(#[case] reaction: bool) {
    let mut runtime = Runtime::new();
    let state = State::new(1);
    let subscription = effect(move |sc| {
        state.get(sc);
    });
    runtime.flush();

    // Pending work owns the last subscription until it runs or is cancelled.
    if reaction {
        Reaction::new(move |_| drop(subscription)).schedule();
    } else {
        Action::new(move |_| drop(subscription)).schedule();
    }

    let result = catch_unwind(AssertUnwindSafe(|| drop(runtime)));

    assert!(
        result.is_ok(),
        "shutdown panicked while releasing a subscription"
    );
}

fn subscribe_to_discardable_signal(discards: Rc<Cell<usize>>) -> Subscription {
    let state = State::new(1);
    let signal = SignalBuilder::from_scan(0, move |value, sc| {
        *value = state.get(sc);
    })
    .on_discard(move |_| discards.set(discards.get() + 1))
    .build();
    effect(move |sc| {
        signal.get(sc);
    })
}

#[test]
fn dropping_subscription_discards_signal_on_flush() {
    let mut runtime = Runtime::new();
    let discards = Rc::new(Cell::new(0));
    let subscription = subscribe_to_discardable_signal(discards.clone());
    runtime.flush();
    assert_eq!(discards.get(), 0);

    drop(subscription);
    runtime.flush();

    assert_eq!(discards.get(), 1);
}

#[rt_local::runtime::blocking::test]
async fn dropping_subscription_wakes_waiting_runtime_for_discard() {
    let mut runtime = Runtime::new();
    let discards = Rc::new(Cell::new(0));
    let subscription = subscribe_to_discardable_signal(discards.clone());
    runtime.flush();
    assert_eq!(discards.get(), 0);
    let (started_sender, started_receiver) = futures::channel::oneshot::channel();

    let main_loop = rt_local::spawn_local(async move {
        let mut lend = runtime.lend();
        started_sender.send(()).unwrap();
        lend.wait_for_ready().await;
        drop(lend);
        runtime.flush();
    });
    // The single-thread executor polls the loop to Pending before resuming us.
    started_receiver.await.unwrap();

    // Another executor task releases a subscription while the main loop sleeps.
    drop(subscription);
    rt_local::wait_for_idle().await;
    let observed_discards = discards.get();

    // Cancel a still-sleeping loop so a missing wake produces failure, not a hang.
    drop(main_loop);
    rt_local::wait_for_idle().await;

    assert_eq!(observed_discards, 1);
}
