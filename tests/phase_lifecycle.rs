use std::{
    cell::{Cell, RefCell},
    future::{Future, pending, poll_fn},
    panic::{AssertUnwindSafe, catch_unwind},
    pin::Pin,
    rc::{Rc, Weak},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

use futures::Stream;
use pretty_assertions::assert_eq;
use sigmut::{
    Action, ActionPhase, Reaction, ReactionPhase, Signal, SignalBuilder, State,
    core::{Runtime, RuntimeConfig},
    effect_async_in, effect_in, spawn_action_async,
};

static A: ReactionPhase = ReactionPhase::new();
static B: ReactionPhase = ReactionPhase::new();
static COMMAND: ActionPhase = ActionPhase::new();
static OTHER: ActionPhase = ActionPhase::new();

#[path = "phase_lifecycle/bridge.rs"]
mod bridge;

fn runtime(phase: &'static ReactionPhase) -> Runtime {
    Runtime::new_with_config(RuntimeConfig::default().with_reaction_phases([phase]))
}
fn ready(runtime: &mut Runtime) -> bool {
    let mut lend = runtime.lend();
    let mut future = Box::pin(lend.wait_for_ready());
    future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
        .is_ready()
}

#[test]
fn configured_order_overrides_identity_allocation_and_preserves_fifo() {
    // Force B and OTHER to acquire identity before A and COMMAND.
    drop(
        RuntimeConfig::default()
            .with_reaction_phases([&B])
            .with_action_phases([&OTHER]),
    );
    let mut runtime = Runtime::new_with_config(
        RuntimeConfig::default()
            .with_reaction_phases([&A, &B])
            .with_action_phases([&COMMAND, &OTHER]),
    );
    let calls = Rc::new(RefCell::new(Vec::new()));
    for (phase, value) in [(&B, 3), (&A, 1), (&A, 2)] {
        Reaction::new({
            let calls = calls.clone();
            move |_| calls.borrow_mut().push(value)
        })
        .schedule_in(phase);
    }
    for (phase, value) in [(&OTHER, 6), (&COMMAND, 4), (&COMMAND, 5)] {
        Action::new({
            let calls = calls.clone();
            move |_| calls.borrow_mut().push(value)
        })
        .schedule_in(phase);
    }
    runtime.dispatch_all_reactions();
    runtime.dispatch_all_actions();
    assert_eq!(*calls.borrow(), [1, 2, 3, 4, 5, 6]);
}

#[test]
fn defaults_are_shared_and_explicit_lists_do_not_add_them() {
    assert!(std::ptr::eq(ActionPhase::DEFAULT, ActionPhase::DEFAULT));
    assert!(std::ptr::eq(ReactionPhase::DEFAULT, ReactionPhase::DEFAULT));
    let mut default = Runtime::new();
    assert!(catch_unwind(|| Action::new(|_| {}).schedule_in(&COMMAND)).is_err());
    let calls = Rc::new(Cell::new(0));
    Reaction::new({
        let calls = calls.clone();
        move |_| calls.set(calls.get() + 1)
    })
    .schedule_in(&A);
    default.flush();
    assert_eq!(calls.get(), 0);
    assert!(!ready(&mut default));
    drop(default);
    let mut explicit = runtime(&A);
    assert!(catch_unwind(|| Action::new(|_| {}).schedule_in(&COMMAND)).is_err());
    Reaction::new({
        let calls = calls.clone();
        move |_| calls.set(calls.get() + 10)
    })
    .schedule();
    explicit.flush();
    assert_eq!(calls.get(), 1);
    assert!(!ready(&mut explicit));
    drop(explicit);
    let mut default = Runtime::new();
    default.flush();
    assert_eq!(calls.get(), 11);
    drop(default);
    let mut empty = Runtime::new_with_config(
        RuntimeConfig::default()
            .with_action_phases([])
            .with_reaction_phases([]),
    );
    assert!(catch_unwind(|| Action::new(|_| {}).schedule()).is_err());
    empty.flush();
    assert!(!ready(&mut empty));
}

#[test]
fn duplicate_phase_registration_is_a_configuration_error() {
    assert!(
        catch_unwind(|| RuntimeConfig::default().with_action_phases([&COMMAND, &COMMAND])).is_err()
    );
    assert!(catch_unwind(|| RuntimeConfig::default().with_reaction_phases([&A, &A])).is_err());
}

#[test]
fn a_b_a_retains_reactions_and_reads_latest_shared_state() {
    let mut first = runtime(&A);
    let state = State::new(0);
    let values = Rc::new(RefCell::new(Vec::new()));
    let _effect = effect_in(&A, {
        let values = values.clone();
        let state = state.clone();
        move |sc| values.borrow_mut().push(state.get(sc))
    });
    first.flush();
    Reaction::new({
        let values = values.clone();
        move |_| values.borrow_mut().push(10)
    })
    .schedule_in(&A);
    drop(first);
    let mut second = runtime(&B);
    state.set(1, second.ac());
    state.set(2, second.ac());
    for value in [3, 4] {
        Reaction::new({
            let values = values.clone();
            move |_| values.borrow_mut().push(value)
        })
        .schedule_in(&A);
    }
    second.flush();
    assert_eq!(*values.borrow(), [0]);
    assert!(!ready(&mut second));
    drop(second);
    let mut third = runtime(&A);
    assert!(ready(&mut third));
    third.flush();
    assert_eq!(*values.borrow(), [0, 10, 2, 3, 4]);
    assert!(!ready(&mut third));
}

#[test]
fn dead_effect_owner_releases_captures_while_phase_sleeps() {
    let capture = Rc::new(());
    let weak = Rc::downgrade(&capture);
    let subscription = effect_in(&A, move |_| {
        let _ = &capture;
    });
    drop(subscription);
    assert!(weak.upgrade().is_none());
    let mut runtime = runtime(&A);
    runtime.flush();
}

#[test]
fn arbitrary_reaction_retains_captures_until_a_matching_runtime() {
    let capture = Rc::new(());
    let weak = Rc::downgrade(&capture);
    Reaction::new(move |_| drop(capture)).schedule_in(&A);
    let mut unrelated = runtime(&B);
    unrelated.flush();
    assert!(weak.upgrade().is_some());
    drop(unrelated);
    let mut matching = runtime(&A);
    matching.flush();
    assert!(weak.upgrade().is_none());
}

#[test]
fn dormant_subscription_unbinds_and_discards_in_another_runtime() {
    let mut first = runtime(&A);
    let discards = Rc::new(Cell::new(0));
    let signal = SignalBuilder::new(|_| 1)
        .on_discard({
            let discards = discards.clone();
            move |_| discards.set(discards.get() + 1)
        })
        .build();
    let subscription = effect_in(&A, move |sc| {
        signal.get(sc);
    });
    first.flush();
    drop(first);
    drop(subscription);
    let mut second = runtime(&B);
    assert!(ready(&mut second));
    second.flush();
    assert_eq!(discards.get(), 1);
    assert!(!ready(&mut second));
}

#[test]
fn actual_discard_flag_is_reset_after_runtime_recreation() {
    let mut runtime = Runtime::new();
    let discards = Rc::new(Cell::new(0));
    let signal = SignalBuilder::new(|_| 1)
        .on_discard({
            let discards = discards.clone();
            move |_| discards.set(discards.get() + 1)
        })
        .build();
    signal.get(&mut runtime.sc());
    drop(runtime);
    let mut runtime = Runtime::new();
    runtime.flush();
    signal.get(&mut runtime.sc());
    runtime.flush();
    assert_eq!(discards.get(), 2);
}

#[test]
fn stream_update_reservation_survives_runtime_recreation() {
    let mut runtime = Runtime::new();
    let state = State::new(1);
    let mut stream = state.to_signal().to_stream();
    let mut cx = Context::from_waker(Waker::noop());
    assert!(Pin::new(&mut stream).poll_next(&mut cx).is_pending());
    drop(runtime);
    runtime = Runtime::new();
    runtime.flush();
    assert_eq!(
        Pin::new(&mut stream).poll_next(&mut cx),
        Poll::Ready(Some(1))
    );
    state.set(2, runtime.ac());
    drop(runtime);
    runtime = Runtime::new();
    runtime.flush();
    assert_eq!(
        Pin::new(&mut stream).poll_next(&mut cx),
        Poll::Ready(Some(2))
    );
    drop(stream);
    runtime.flush();
}

#[rstest::rstest]
fn persistent_async_wake_survives_unrelated_runtime(#[values(false, true)] stream: bool) {
    let mut first = runtime(&A);
    let values = Rc::new(RefCell::new(Vec::new()));
    let (sender, receiver) = futures::channel::oneshot::channel::<i32>();
    let signal = if stream {
        Signal::from_stream(futures::stream::once(
            async move { receiver.await.unwrap() },
        ))
    } else {
        Signal::from_future(async move { receiver.await.unwrap() })
    };
    let subscription = effect_in(&A, {
        let values = values.clone();
        move |sc| values.borrow_mut().push(signal.get(sc))
    });
    first.flush();
    drop(first);
    std::thread::spawn(move || sender.send(42).unwrap())
        .join()
        .unwrap();
    let mut second = runtime(&B);
    second.flush();
    assert_eq!(*values.borrow(), [Poll::Pending]);
    assert!(!ready(&mut second));
    drop(second);
    let mut third = runtime(&A);
    third.flush();
    assert_eq!(*values.borrow(), [Poll::Pending, Poll::Ready(42)]);
    drop(subscription);
    third.flush();
}

#[test]
fn effect_async_first_poll_and_continuation_survive_shutdown() {
    let polls = Rc::new(Cell::new(0));
    let wake = Rc::new(RefCell::new(None));
    let subscription = effect_async_in(&A, {
        let polls = polls.clone();
        let wake = wake.clone();
        async move |_| {
            poll_fn(|cx| {
                polls.set(polls.get() + 1);
                *wake.borrow_mut() = Some(cx.waker().clone());
                Poll::<()>::Pending
            })
            .await;
        }
    });
    let mut first = runtime(&B);
    first.flush();
    drop(first);
    let mut first = runtime(&A);
    first.flush();
    assert_eq!(polls.get(), 1);
    first.begin_shutdown();
    wake.borrow().as_ref().unwrap().wake_by_ref();
    first.flush();
    assert_eq!(polls.get(), 1);
    drop(first);
    let mut second = runtime(&A);
    second.flush();
    assert_eq!(polls.get(), 2);
    drop(subscription);
    second.flush();
}

#[test]
fn closing_cancels_actions_without_polling_and_keeps_binding() {
    let mut runtime = Runtime::new();
    let capture = Rc::new(());
    let weak = Rc::downgrade(&capture);
    Action::new(move |_| drop(capture)).schedule();
    let polls = Rc::new(Cell::new(0));
    runtime.ac().start_async_inline({
        let polls = polls.clone();
        async move |_| {
            poll_fn(|_| {
                polls.set(polls.get() + 1);
                Poll::<()>::Pending
            })
            .await;
        }
    });
    runtime.begin_shutdown();
    runtime.begin_shutdown();
    assert!(weak.upgrade().is_none());
    assert_eq!(polls.get(), 1);
    Action::new(|_| panic!("Closing executed an Action")).schedule_in(&OTHER);
    spawn_action_async(async |_| panic!("Closing polled a new action"));
    runtime
        .ac()
        .start_async_inline_in(&OTHER, async |_| panic!("Closing polled inline"));
    assert!(!runtime.dispatch_action(&OTHER));
    assert!(!runtime.dispatch_actions(&OTHER));
    let reactions = Rc::new(Cell::new(0));
    Reaction::new({
        let reactions = reactions.clone();
        move |_| reactions.set(reactions.get() + 1)
    })
    .schedule_in(&A);
    runtime.flush();
    assert!(!ready(&mut runtime));
    assert!(catch_unwind(Runtime::new).is_err());
    drop(runtime);
    assert!(catch_unwind(|| Action::new(|_| {}).schedule()).is_err());
    assert_eq!(reactions.get(), 0);
    let mut next = self::runtime(&A);
    next.flush();
    assert_eq!(reactions.get(), 1);
}

#[rstest::rstest]
fn closing_signal_reads_do_not_poll_user_futures(
    #[values("future", "stream", "async")] kind: &str,
) {
    let mut runtime = Runtime::new();
    runtime.begin_shutdown();
    let polls = Rc::new(Cell::new(0));
    let poll = {
        let polls = polls.clone();
        move || {
            polls.set(polls.get() + 1);
            Poll::<i32>::Pending
        }
    };
    let signal = match kind {
        "future" => Signal::from_future(poll_fn(move |_| poll())),
        "stream" => Signal::from_stream(futures::stream::poll_fn(move |_| poll().map(Some))),
        "async" => Signal::from_async(async move |_| poll_fn(|_| poll()).await),
        _ => unreachable!(),
    };
    assert_eq!(signal.get(&mut runtime.sc()), Poll::Pending);
    assert_eq!(polls.get(), 0);
    drop(runtime);
    let mut next = Runtime::new();
    assert_eq!(signal.get(&mut next.sc()), Poll::Pending);
    assert_eq!(polls.get(), 1);
    drop(signal);
    next.flush();
}

struct DuringDrop(Option<Box<dyn FnOnce()>>);
impl Drop for DuringDrop {
    fn drop(&mut self) {
        self.0.take().unwrap()();
    }
}

#[rstest::rstest]
fn rejected_action_captures_drop_outside_scheduler_borrow(
    #[values("absent", "running", "closing")] status: &str,
) {
    let mut runtime = if status == "absent" {
        None
    } else {
        Some(Runtime::new())
    };
    if status == "closing" {
        runtime.as_mut().unwrap().begin_shutdown();
    }
    let calls = Rc::new(RefCell::new(Vec::new()));
    let guard = DuringDrop(Some(Box::new({
        let calls = calls.clone();
        move || {
            calls.borrow_mut().push("drop");
            Reaction::new(move |_| calls.borrow_mut().push("reaction")).schedule();
        }
    })));
    let result = catch_unwind(AssertUnwindSafe(|| {
        Action::new(move |_| {
            let _guard = guard;
            panic!("rejected Action ran");
        })
        .schedule_in(&OTHER);
    }));
    assert_eq!(result.is_err(), status != "closing");
    assert_eq!(*calls.borrow(), ["drop"]);
    drop(runtime);
    let mut next = Runtime::new();
    next.flush();
    assert_eq!(*calls.borrow(), ["drop", "reaction"]);
}

#[test]
fn reaction_panic_does_not_destroy_later_effect_reservations() {
    let mut first = Runtime::new();
    Reaction::new(|_| panic!("callback failed")).schedule();
    let calls = Rc::new(Cell::new(0));
    let subscription = sigmut::effect({
        let calls = calls.clone();
        move |_| calls.set(calls.get() + 1)
    });
    assert!(catch_unwind(AssertUnwindSafe(|| first.dispatch_all_reactions())).is_err());
    drop(first);
    let mut second = Runtime::new();
    second.flush();
    assert_eq!(calls.get(), 1);
    drop(subscription);
    second.flush();
}

#[test]
fn action_panic_keeps_unexecuted_captures_until_closing() {
    let mut runtime = Runtime::new();
    let drops = Rc::new(Cell::new(0));
    Action::new(|_| panic!("callback failed")).schedule();
    let guard = DuringDrop(Some(Box::new({
        let drops = drops.clone();
        move || {
            drops.set(drops.get() + 1);
            Action::new(|_| panic!("action from cancelled capture ran")).schedule();
        }
    })));
    Action::new(move |_| drop(guard)).schedule();
    assert!(catch_unwind(AssertUnwindSafe(|| runtime.dispatch_all_actions())).is_err());
    assert_eq!(drops.get(), 0);
    runtime.begin_shutdown();
    runtime.flush();
    assert_eq!(drops.get(), 1);
}

#[test]
fn queued_capture_drop_reenters_closing_and_cancels_completion_sender() {
    let mut runtime = Runtime::new();
    let reactions = Rc::new(Cell::new(0));
    let guard = DuringDrop(Some(Box::new({
        let reactions = reactions.clone();
        move || {
            Action::new(|_| panic!("Action from capture Drop ran")).schedule();
            Reaction::new(move |_| reactions.set(reactions.get() + 1)).schedule();
        }
    })));
    let (sender, mut receiver) = futures::channel::oneshot::channel::<()>();
    Action::new(move |_| {
        let _guard = guard;
        let _ = sender.send(());
    })
    .schedule();
    runtime.begin_shutdown();
    assert!(matches!(
        Pin::new(&mut receiver).poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Err(_))
    ));
    runtime.flush();
    assert_eq!(reactions.get(), 0);
    drop(runtime);
    let mut next = Runtime::new();
    next.flush();
    assert_eq!(reactions.get(), 1);
}

#[rstest::rstest]
fn cancellation_drop_reenters_closing_and_preserves_notifications(
    #[values(false, true)] explicit: bool,
) {
    let mut runtime = Runtime::new();
    let state = State::new(0);
    let values = Rc::new(RefCell::new(Vec::new()));
    let _subscription = sigmut::effect({
        let state = state.clone();
        let values = values.clone();
        move |sc| values.borrow_mut().push(state.get(sc))
    });
    runtime.flush();
    runtime.ac().start_async_inline(async move |aac| {
        let _guard = DuringDrop(Some(Box::new(move || {
            aac.call(|ac| {
                state.set(42, ac);
                ac.start_async_inline(async |_| panic!("started during cancellation"));
            });
            Action::new(|_| panic!("ran during cancellation")).schedule();
            Reaction::new(|_| {}).schedule();
        })));
        pending::<()>().await;
    });
    if explicit {
        runtime.begin_shutdown();
        runtime.flush();
    }
    drop(runtime);
    let mut runtime = Runtime::new();
    runtime.flush();
    assert_eq!(*values.borrow(), [0, 42]);
}

#[test]
fn old_action_wake_and_context_do_not_connect_to_new_session() {
    let mut first = Runtime::new();
    let old_wake = Rc::new(RefCell::new(None));
    let old_context = Rc::new(RefCell::new(None));
    first.ac().start_async_inline({
        let old_wake = old_wake.clone();
        let old_context = old_context.clone();
        async move |aac| {
            *old_context.borrow_mut() = Some(aac);
            poll_fn(|cx| {
                *old_wake.borrow_mut() = Some(cx.waker().clone());
                Poll::<()>::Pending
            })
            .await;
        }
    });
    drop(first);
    let mut second = Runtime::new();
    old_wake.borrow().as_ref().unwrap().wake_by_ref();
    second.flush();
    assert!(!ready(&mut second));
    assert!(
        catch_unwind(AssertUnwindSafe(|| old_context
            .borrow()
            .as_ref()
            .unwrap()
            .call(|_| ())))
        .is_err()
    );
}

struct CountWake(AtomicUsize);
impl Wake for CountWake {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn cancelled_action_wake_does_not_wake_next_driver() {
    let mut first =
        Runtime::new_with_config(RuntimeConfig::default().with_action_phases([&COMMAND, &OTHER]));
    let old_wake = Rc::new(RefCell::new(None));
    first.ac().start_async_inline_in(&COMMAND, {
        let old_wake = old_wake.clone();
        async move |_| {
            poll_fn(|cx| {
                *old_wake.borrow_mut() = Some(cx.waker().clone());
                Poll::<()>::Pending
            })
            .await;
        }
    });
    // Leave a queued request while cancellation retires the registration.
    old_wake.borrow().as_ref().unwrap().wake_by_ref();
    drop(first);
    let mut second =
        Runtime::new_with_config(RuntimeConfig::default().with_action_phases([&OTHER, &COMMAND]));
    let polls = Rc::new(Cell::new(0));
    second.ac().start_async_inline_in(&OTHER, {
        let polls = polls.clone();
        async move |_| {
            poll_fn(|_| {
                polls.set(polls.get() + 1);
                Poll::<()>::Pending
            })
            .await;
        }
    });
    let count = Arc::new(CountWake(AtomicUsize::new(0)));
    let waker = Waker::from(count.clone());
    {
        let mut lend = second.lend();
        let mut wait = Box::pin(lend.wait_for_ready());
        assert!(
            wait.as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_pending()
        );
    }
    let late = old_wake.borrow().as_ref().unwrap().clone();
    std::thread::spawn(move || late.wake()).join().unwrap();
    assert_eq!(count.0.load(Ordering::Relaxed), 0);
    second.flush();
    assert_eq!(polls.get(), 1);
}

struct DriverWake(Arc<AtomicUsize>);
impl Wake for DriverWake {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
        // Reentry fails if the caller holds the scheduler's TLS borrow.
        Reaction::new(|_| {}).schedule();
    }
}
impl Drop for DriverWake {
    fn drop(&mut self) {
        Reaction::new(|_| {}).schedule();
    }
}

#[test]
fn driver_wake_and_drop_reenter_outside_scheduler_borrows() {
    let mut runtime = Runtime::new();
    let wakes = Arc::new(AtomicUsize::new(0));
    let waker = Waker::from(Arc::new(DriverWake(wakes.clone())));
    {
        let mut lend = runtime.lend();
        let mut wait = Box::pin(lend.wait_for_ready());
        assert!(
            wait.as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_pending()
        );
    }
    Reaction::new(|_| {}).schedule();
    assert_eq!(wakes.load(Ordering::Relaxed), 1);
    runtime.flush();
    {
        let mut lend = runtime.lend();
        let mut wait = Box::pin(lend.wait_for_ready());
        assert!(
            wait.as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_pending()
        );
    }
    drop(waker);
    runtime.begin_shutdown();
    assert_eq!(wakes.load(Ordering::Relaxed), 1);
    drop(runtime);
    let mut runtime = Runtime::new();
    runtime.flush();
}
