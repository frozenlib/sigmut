static ACTION_NEGATIVE: ActionPhase = ActionPhase::new();
static ACTION_ONE: ActionPhase = ActionPhase::new();
static ACTION_TWO: ActionPhase = ActionPhase::new();
static REACTION_ONE: ReactionPhase = ReactionPhase::new();
static REACTION_TWO: ReactionPhase = ReactionPhase::new();

fn runtime() -> Runtime {
    Runtime::new_with_config(
        RuntimeConfig::default()
            .with_action_phases([
                ActionPhase::DEFAULT,
                &ACTION_NEGATIVE,
                &ACTION_ONE,
                &ACTION_TWO,
            ])
            .with_reaction_phases([ReactionPhase::DEFAULT, &REACTION_TWO]),
    )
}

use super::*;
mod source_bindings;
use crate::utils::sync::oneshot_broadcast;
use pretty_assertions::assert_eq;
use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

fn schedule_call(phase: &'static ActionPhase, calls: Rc<RefCell<Vec<i32>>>, value: i32) {
    Action::new(move |_| calls.borrow_mut().push(value)).schedule_in(phase);
}

fn schedule_reaction_call(phase: &'static ReactionPhase, calls: Rc<RefCell<Vec<i32>>>, value: i32) {
    Reaction::new(move |_| calls.borrow_mut().push(value)).schedule_in(phase);
}

#[test]
fn discard_reaction_shares_node_and_releases_it_after_call() {
    struct Node(Rc<Cell<usize>>);

    impl DiscardReaction for Node {
        fn discard(self: Rc<Self>, _: &mut ReactionContext<'_, '_>) {
            self.0.set(self.0.get() + 1);
        }
    }

    let mut rt = runtime();
    let calls = Rc::new(Cell::new(0));
    let node = Rc::new(Node(calls.clone()));
    let reaction = Reaction::from_discard(node.clone());
    let RawReaction::Discard(erased) = &reaction.0 else {
        std::panic!("discard reaction must share the node");
    };
    assert!(std::ptr::eq(
        Rc::as_ptr(&node),
        Rc::as_ptr(erased).cast::<Node>()
    ));
    assert_eq!(Rc::strong_count(&node), 2);
    reaction.run(&mut rt.rc());
    assert_eq!((calls.get(), Rc::strong_count(&node)), (1, 1));
}

#[test]
fn sink_bindings_update_does_not_downgrade_dirty() {
    struct NoopSink;

    impl BindSink for NoopSink {
        fn notify(self: Rc<Self>, _slot: Slot, _level: DirtyLevel, _nc: &mut NotifyContext) {}
    }

    let sink: Rc<dyn BindSink> = Rc::new(NoopSink);
    let mut sinks = SinkBindings::new();
    let key = BindKey(sinks.0.insert(SinkBinding {
        sink: Rc::downgrade(&sink),
        slot: Slot(0),
        dirty: Dirty::Dirty,
    }));
    let mut rt = runtime();
    let mut rc = rt.rc();

    sinks.update(false, &mut rc);

    assert!(sinks.is_dirty(key, &mut rc));
}

#[test]
fn runtime_with_config_accepts_configured_phases() {
    let action_phase = &ACTION_NEGATIVE;
    let reaction_phase = &REACTION_TWO;
    let mut rt = Runtime::new_with_config(
        RuntimeConfig::default()
            .with_action_phases([action_phase])
            .with_reaction_phases([reaction_phase]),
    );
    let calls = Rc::new(RefCell::new(Vec::new()));

    schedule_call(action_phase, calls.clone(), 1);
    schedule_reaction_call(reaction_phase, calls.clone(), 2);

    assert!(rt.dispatch_action(action_phase));
    assert!(rt.dispatch_reactions(reaction_phase));
    assert_eq!(*calls.borrow(), [1, 2]);
}

#[test]
fn runtime_with_config_rejects_invalid_schedules_before_queueing() {
    let valid_action_phase = &ACTION_ONE;
    let valid_reaction_phase = &REACTION_ONE;
    let mut rt = Runtime::new_with_config(
        RuntimeConfig::default()
            .with_action_phases([valid_action_phase])
            .with_reaction_phases([valid_reaction_phase]),
    );

    assert!(catch_unwind(|| Action::new(|_| {}).schedule_in(&ACTION_TWO)).is_err());
    Reaction::new(|_| {}).schedule_in(&REACTION_TWO);
    assert!(catch_unwind(|| spawn_action_async_in(&ACTION_TWO, async |_| {})).is_err());
    assert!(!rt.dispatch_all_actions());
    assert!(!rt.dispatch_all_reactions());
}

#[test]
fn runtime_with_config_rejects_invalid_dispatches_when_queues_are_empty() {
    let mut rt = Runtime::new_with_config(
        RuntimeConfig::default()
            .with_action_phases([ActionPhase::DEFAULT])
            .with_reaction_phases([ReactionPhase::DEFAULT]),
    );

    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            rt.dispatch_action(&ACTION_ONE);
        }))
        .is_err()
    );
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            rt.dispatch_actions(&ACTION_ONE);
        }))
        .is_err()
    );
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            rt.dispatch_reactions(&REACTION_ONE);
        }))
        .is_err()
    );
}

#[test]
fn runtime_with_config_accepts_valid_phases_scheduled_before_creation() {
    let action_phase = &ACTION_ONE;
    let reaction_phase = &REACTION_TWO;
    let calls = Rc::new(RefCell::new(Vec::new()));
    schedule_reaction_call(reaction_phase, calls.clone(), 3);
    schedule_reaction_call(reaction_phase, calls.clone(), 4);

    let mut rt = Runtime::new_with_config(
        RuntimeConfig::default()
            .with_action_phases([action_phase])
            .with_reaction_phases([reaction_phase]),
    );

    schedule_call(action_phase, calls.clone(), 1);
    schedule_call(action_phase, calls.clone(), 2);
    assert!(rt.dispatch_actions(action_phase));
    assert!(rt.dispatch_reactions(reaction_phase));
    assert_eq!(*calls.borrow(), [1, 2, 3, 4]);
}

#[test]
fn runtime_config_remains_active_during_runtime_call() {
    let phase = &ACTION_ONE;
    let mut rt = Runtime::new_with_config(RuntimeConfig::default().with_action_phases([phase]));
    {
        let _lend = rt.lend();
        Runtime::call(|_| Action::new(|_| {}).schedule_in(phase));
        assert!(
            catch_unwind(|| {
                Runtime::call(|_| Action::new(|_| {}).schedule_in(ActionPhase::DEFAULT));
            })
            .is_err()
        );
        Runtime::call(|_| {});
    }

    assert!(rt.dispatch_action(phase));
    assert!(!rt.dispatch_action(phase));
}

#[test]
fn runtime_config_applies_when_an_async_action_wakes() {
    let phase = &ACTION_ONE;
    let mut rt = Runtime::new_with_config(RuntimeConfig::default().with_action_phases([phase]));
    let (sender, receiver) = oneshot_broadcast();
    let called = Rc::new(Cell::new(false));
    spawn_action_async_in(phase, {
        let called = called.clone();
        async move |context| {
            receiver.recv().await;
            context.call(|_| called.set(true));
        }
    });

    assert!(rt.dispatch_action(phase));
    assert!(!called.get());
    sender.send(());
    assert!(rt.dispatch_action(phase));
    assert!(called.get());
}

#[test]
fn async_action_inline_ready_updates_state_and_releases_capture_before_return() {
    let mut rt = runtime();
    let state = crate::State::new(0);
    let capture = Rc::new(());
    let weak = Rc::downgrade(&capture);
    rt.ac().start_async_inline({
        let state = state.clone();
        async move |aac| {
            assert!(aac.is_running_inline());
            let value = std::future::ready(42).await;
            assert!(aac.is_running_inline());
            aac.call(|ac| state.set(value, ac));
            let _capture = capture;
        }
    });

    assert_eq!(state.get(&mut rt.sc()), 42);
    assert!(weak.upgrade().is_none());
    assert!(!rt.dispatch_all_actions());
}

#[test]
fn async_action_inline_resumes_only_in_requested_phase() {
    let phase = &ACTION_ONE;
    let mut rt = runtime();
    let (first_sender, first_receiver) = oneshot_broadcast();
    let (second_sender, second_receiver) = oneshot_broadcast();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let state = crate::State::new(0);
    rt.ac().start_async_inline_in(phase, {
        let calls = calls.clone();
        let state = state.clone();
        async move |aac| {
            calls.borrow_mut().push(aac.is_running_inline());
            aac.call(|ac| state.set(1, ac));
            first_receiver.recv().await;
            calls.borrow_mut().push(aac.is_running_inline());
            aac.call(|ac| state.set(2, ac));
            second_receiver.recv().await;
            calls.borrow_mut().push(aac.is_running_inline());
            aac.call(|ac| state.set(3, ac));
        }
    });
    assert_eq!(state.get(&mut rt.sc()), 1);
    assert!(!rt.dispatch_all_actions());
    first_sender.send(());
    assert!(!rt.dispatch_actions(ActionPhase::DEFAULT));
    assert_eq!(*calls.borrow(), [true]);
    assert!(rt.dispatch_action(phase));
    assert_eq!(state.get(&mut rt.sc()), 2);
    second_sender.send(());
    assert!(!rt.dispatch_actions(ActionPhase::DEFAULT));
    assert!(rt.dispatch_action(phase));
    assert_eq!(*calls.borrow(), [true, false, false]);
    assert_eq!(state.get(&mut rt.sc()), 3);
}

#[test]
fn async_action_inline_self_wake_defers_repoll_and_preserves_queued_actions() {
    let mut rt = runtime();
    let polls = Rc::new(Cell::new(0));
    let calls = Rc::new(RefCell::new(Vec::new()));
    schedule_call(ActionPhase::DEFAULT, calls.clone(), 1);
    rt.ac().start_async_inline({
        let polls = polls.clone();
        async move |aac| {
            poll_fn(|cx| {
                polls.set(polls.get() + 1);
                match polls.get() {
                    1 => {
                        assert!(aac.is_running_inline());
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                    2 => {
                        assert!(!aac.is_running_inline());
                        Poll::Ready(())
                    }
                    _ => std::panic!("completed handler was polled again"),
                }
            })
            .await;
        }
    });

    assert_eq!(polls.get(), 1);
    assert_eq!(*calls.borrow(), Vec::<i32>::new());
    assert!(rt.dispatch_action(ActionPhase::DEFAULT));
    assert_eq!(*calls.borrow(), [1]);
    assert_eq!(polls.get(), 1);
    assert!(rt.dispatch_action(ActionPhase::DEFAULT));
    assert_eq!(polls.get(), 2);
}

#[test]
fn async_action_inline_runtime_owns_pending_future_without_external_waker() {
    let mut rt = runtime();
    let capture = Rc::new(());
    let weak = Rc::downgrade(&capture);
    rt.ac().start_async_inline(async move |_| {
        std::future::pending::<()>().await;
        drop(capture);
    });

    assert_eq!(weak.strong_count(), 1);
    drop(rt);
    assert!(weak.upgrade().is_none());
}

#[rstest::rstest]
fn async_action_inline_late_wakes_do_not_repoll(#[values(false, true)] complete: bool) {
    let mut rt = runtime();
    let waker = Rc::new(RefCell::new(None));
    let polls = Rc::new(Cell::new(0));
    let capture = Rc::new(());
    let weak = Rc::downgrade(&capture);
    rt.ac().start_async_inline({
        let waker = waker.clone();
        let polls = polls.clone();
        async move |_| {
            poll_fn(|cx| {
                polls.set(polls.get() + 1);
                *waker.borrow_mut() = Some(cx.waker().clone());
                if complete {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            })
            .await;
            drop(capture);
        }
    });
    if complete {
        assert!(weak.upgrade().is_none());
        waker.borrow().as_ref().unwrap().wake_by_ref();
        waker.borrow().as_ref().unwrap().wake_by_ref();
        rt.dispatch_all_actions();
        assert_eq!(polls.get(), 1);
    }
    drop(rt);
    assert!(weak.upgrade().is_none());
    waker.borrow().as_ref().unwrap().wake_by_ref();
    let mut rt = runtime();
    rt.dispatch_all_actions();
    assert_eq!(polls.get(), 1);
}

#[rstest::rstest]
fn async_action_inline_rejects_invalid_phase_before_handler(#[values(false, true)] explicit: bool) {
    let mut rt =
        Runtime::new_with_config(RuntimeConfig::default().with_action_phases([&ACTION_ONE]));
    let called = Rc::new(Cell::new(false));
    let capture = Rc::new(());
    let weak = Rc::downgrade(&capture);
    let handler = {
        let called = called.clone();
        async move |_| {
            called.set(true);
            drop(capture);
        }
    };
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            if explicit {
                rt.ac().start_async_inline_in(&ACTION_TWO, handler);
            } else {
                rt.ac().start_async_inline(handler);
            }
        }))
        .is_err()
    );

    assert!(!called.get());
    assert!(weak.upgrade().is_none());
    assert!(!rt.dispatch_all_actions());
}

#[rstest::rstest]
fn async_action_spawn_is_deferred_and_never_inline(#[values(false, true)] explicit: bool) {
    let phase = if explicit {
        &ACTION_ONE
    } else {
        ActionPhase::DEFAULT
    };
    let mut rt = runtime();
    let (sender, receiver) = oneshot_broadcast();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let handler = {
        let calls = calls.clone();
        async move |aac: &mut AsyncActionContext| {
            calls.borrow_mut().push(aac.is_running_inline());
            receiver.recv().await;
            calls.borrow_mut().push(aac.is_running_inline());
        }
    };
    if explicit {
        spawn_action_async_in(phase, handler);
    } else {
        spawn_action_async(handler);
    }
    assert_eq!(*calls.borrow(), Vec::<bool>::new());
    assert!(rt.dispatch_action(phase));
    assert_eq!(*calls.borrow(), [false]);
    sender.send(());
    assert!(rt.dispatch_action(phase));
    assert_eq!(*calls.borrow(), [false, false]);
}

#[test]
fn async_action_inline_invocations_are_independent() {
    let mut rt = runtime();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut senders = Vec::new();
    for (id, phase) in [(1, &ACTION_ONE), (2, &ACTION_TWO)] {
        let (sender, receiver) = oneshot_broadcast();
        senders.push(sender);
        rt.ac().start_async_inline_in(phase, {
            let calls = calls.clone();
            async move |aac| {
                assert!(aac.is_running_inline());
                receiver.recv().await;
                assert!(!aac.is_running_inline());
                calls.borrow_mut().push(id);
            }
        });
    }
    for sender in senders {
        sender.send(());
    }
    assert!(rt.dispatch_action(&ACTION_TWO));
    assert_eq!(*calls.borrow(), [2]);
    assert!(rt.dispatch_action(&ACTION_ONE));
    assert_eq!(*calls.borrow(), [2, 1]);
}

#[test]
fn async_action_inline_nested_start_preserves_outer_status() {
    let mut rt = runtime();
    let (sender, receiver) = oneshot_broadcast();
    let calls = Rc::new(RefCell::new(Vec::new()));
    rt.ac().start_async_inline({
        let calls = calls.clone();
        async move |outer| {
            for iteration in 0..2 {
                let expected = iteration == 0;
                assert_eq!(outer.is_running_inline(), expected);
                outer.call(|ac| {
                    ac.start_async_inline({
                        let calls = calls.clone();
                        async move |inner| {
                            assert!(inner.is_running_inline());
                            inner.call(|_| calls.borrow_mut().push(iteration));
                        }
                    });
                });
                assert_eq!(outer.is_running_inline(), expected);
                if iteration == 0 {
                    receiver.recv().await;
                }
            }
        }
    });
    assert_eq!(*calls.borrow(), [0]);
    sender.send(());
    rt.dispatch_all_actions();
    assert_eq!(*calls.borrow(), [0, 1]);
}

#[rstest::rstest]
fn async_action_inline_status_is_false_outside_poll(
    #[values("ready", "pending", "panic")] outcome: &str,
) {
    let mut rt = runtime();
    let context = Rc::new(RefCell::new(None));
    let should_panic = outcome == "panic";
    let outcome = outcome.to_owned();
    let result = catch_unwind(AssertUnwindSafe(|| {
        rt.ac().start_async_inline({
            let context = context.clone();
            async move |aac| {
                assert!(aac.is_running_inline());
                *context.borrow_mut() = Some(aac);
                match outcome.as_str() {
                    "ready" => {}
                    "pending" => std::future::pending::<()>().await,
                    "panic" => std::panic!("handler panic"),
                    _ => unreachable!(),
                }
            }
        });
    }));
    assert_eq!(result.is_err(), should_panic);
    assert!(!context.borrow().as_ref().unwrap().is_running_inline());
    drop(rt);
    assert!(!context.borrow().as_ref().unwrap().is_running_inline());
}

#[test]
fn runtime_creation_rejects_a_second_runtime_without_corrupting_the_first() {
    let phase = &ACTION_ONE;
    let mut rt = Runtime::new_with_config(RuntimeConfig::default().with_action_phases([phase]));

    assert!(catch_unwind(Runtime::new).is_err());
    Action::new(|_| {}).schedule_in(phase);
    assert!(rt.dispatch_action(phase));
}

#[test]
fn dispatch_action_runs_one_action_in_fifo_order() {
    let mut rt = runtime();
    let phase = &ACTION_ONE;
    let calls = Rc::new(RefCell::new(Vec::new()));
    for value in [1, 2] {
        schedule_call(phase, calls.clone(), value);
    }

    assert!(rt.dispatch_action(phase));
    assert_eq!(*calls.borrow(), [1]);
    assert!(rt.dispatch_action(phase));
    assert_eq!(*calls.borrow(), [1, 2]);
    assert!(!rt.dispatch_action(phase));
}

#[test]
fn dispatch_action_only_runs_the_requested_phase() {
    let mut rt = runtime();
    let first_phase = &ACTION_ONE;
    let second_phase = &ACTION_TWO;
    let calls = Rc::new(RefCell::new(Vec::new()));
    for (phase, value) in [(first_phase, 1), (second_phase, 2)] {
        schedule_call(phase, calls.clone(), value);
    }

    assert!(rt.dispatch_action(second_phase));
    assert_eq!(*calls.borrow(), [2]);
    assert!(!rt.dispatch_action(second_phase));
    assert!(rt.dispatch_action(first_phase));
    assert_eq!(*calls.borrow(), [2, 1]);
}

#[test]
fn dispatch_action_defers_actions_scheduled_in_the_same_phase() {
    let mut rt = runtime();
    let phase = &ACTION_ONE;
    let calls = Rc::new(RefCell::new(Vec::new()));
    Action::new({
        let calls = calls.clone();
        move |_| {
            calls.borrow_mut().push(1);
            schedule_call(phase, calls.clone(), 2);
        }
    })
    .schedule_in(phase);

    assert!(rt.dispatch_action(phase));
    assert_eq!(*calls.borrow(), [1]);
    assert!(rt.dispatch_action(phase));
    assert_eq!(*calls.borrow(), [1, 2]);
}

#[test]
fn dispatch_actions_retain_batch_semantics() {
    let mut rt = runtime();
    let phase = &ACTION_ONE;
    let other_phase = &ACTION_TWO;
    let calls = Rc::new(RefCell::new(Vec::new()));
    Action::new({
        let calls = calls.clone();
        move |_| {
            calls.borrow_mut().push(1);
            schedule_call(phase, calls.clone(), 2);
        }
    })
    .schedule_in(phase);
    Action::new({
        let calls = calls.clone();
        move |_| {
            calls.borrow_mut().push(3);
            schedule_call(other_phase, calls.clone(), 4);
        }
    })
    .schedule_in(other_phase);

    assert!(rt.dispatch_actions(phase));
    assert_eq!(*calls.borrow(), [1, 2]);
    assert!(rt.dispatch_all_actions());
    assert_eq!(*calls.borrow(), [1, 2, 3, 4]);
}

#[test]
fn action_from_weak_fn_runs_when_alive() {
    let mut rt = runtime();
    let flag = Rc::new(Cell::new(false));
    let weak = Rc::downgrade(&flag);
    Action::from_weak_fn(weak, |flag, _| flag.set(true)).schedule();
    rt.dispatch_all_actions();
    assert!(flag.get());
}

#[test]
fn runtime_call_within_lend() {
    let mut rt = runtime();
    let _lend = rt.lend();
    Runtime::call(|_rt| {});
}

#[test]
fn runtime_call_within_lend_2() {
    let mut rt = runtime();
    let _lend = rt.lend();
    Runtime::call(|_rt| {});
    Runtime::call(|_rt| {});
}

#[test]
fn runtime_try_call_within_lend() {
    let mut rt = runtime();
    let _lend = rt.lend();
    assert_eq!(Runtime::try_call(|_rt| 1), Ok(1));
}

#[test]
fn runtime_try_call_without_runtime() {
    assert_eq!(
        Runtime::try_call(|_rt| ()),
        Err(RuntimeCallError::RuntimeDoesNotExist)
    );
}

#[test]
fn runtime_try_call_outside_lend() {
    let _rt = Runtime::new();
    assert_eq!(
        Runtime::try_call(|_rt| ()),
        Err(RuntimeCallError::RuntimeUnavailable)
    );
}

#[test]
#[should_panic(expected = "Runtime is not available")]
fn runtime_call_outside_lend() {
    let _rt = Runtime::new();
    Runtime::call(|_rt| {});
}

#[test]
#[should_panic(expected = "Runtime does not exist")]
fn runtime_call_without_runtime() {
    Runtime::call(|_rt| {});
}

#[test]
fn flush_settles_actions_created_by_carried_maintenance() {
    struct Sink(Rc<Cell<usize>>);
    impl BindSink for Sink {
        fn notify(self: Rc<Self>, _: Slot, _: DirtyLevel, _: &mut NotifyContext) {
            Action::new(move |_| self.0.set(self.0.get() + 1)).schedule();
        }
    }
    let first = Runtime::new();
    let calls = Rc::new(Cell::new(0));
    let sink: Rc<dyn BindSink> = Rc::new(Sink(calls.clone()));
    schedule_notify(Rc::downgrade(&sink), Slot(0));
    drop(first);
    let mut second = Runtime::new();
    second.flush();
    assert_eq!(calls.get(), 1);
}

#[rstest::rstest]
fn driver_callbacks_release_both_tls_and_request_mutex(
    #[values("schedule", "wake", "replace", "shutdown")] operation: &str,
) {
    use std::sync::atomic::AtomicBool;
    struct Probe {
        requests: WakeRequests,
        outside: Arc<AtomicBool>,
    }
    impl Probe {
        fn check(&self) {
            let tls_free = GLOBALS.with(|g| g.try_borrow_mut().is_ok());
            let mutex_free = self.requests.0.try_lock().is_ok();
            self.outside
                .fetch_and(tls_free && mutex_free, Ordering::Relaxed);
        }
    }
    impl Wake for Probe {
        fn wake(self: Arc<Self>) {
            self.check();
        }
    }
    impl Drop for Probe {
        fn drop(&mut self) {
            self.check();
        }
    }
    struct Noop;
    impl BindSink for Noop {
        fn notify(self: Rc<Self>, _: Slot, _: DirtyLevel, _: &mut NotifyContext) {}
    }
    let mut runtime = Runtime::new();
    let outside = Arc::new(AtomicBool::new(true));
    let requests = Globals::with(|g| g.wakes.requests.clone());
    let waker = Waker::from(Arc::new(Probe {
        requests,
        outside: outside.clone(),
    }));
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
    match operation {
        "schedule" => Reaction::new(|_| {}).schedule(),
        "wake" => waker_from_sink(Rc::downgrade(&Rc::new(Noop)), Slot(0)).wake(),
        "replace" => {
            let mut lend = runtime.lend();
            let mut wait = Box::pin(lend.wait_for_ready());
            assert!(
                wait.as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
        }
        "shutdown" => runtime.begin_shutdown(),
        _ => unreachable!(),
    }
    assert!(outside.load(Ordering::Relaxed));
    drop(runtime);
    let mut runtime = Runtime::new();
    runtime.flush();
}
