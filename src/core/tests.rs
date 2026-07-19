use super::*;
use crate::utils::sync::oneshot_broadcast;
use pretty_assertions::assert_eq;
use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

fn schedule_call(phase: ActionPhase, calls: Rc<RefCell<Vec<i32>>>, value: i32) {
    Action::new(move |_| calls.borrow_mut().push(value)).schedule_in(phase);
}

fn schedule_reaction_call(phase: ReactionPhase, calls: Rc<RefCell<Vec<i32>>>, value: i32) {
    Reaction::new(move |_| calls.borrow_mut().push(value)).schedule_in(phase);
}

#[test]
fn runtime_config_phase_sets_cover_the_full_id_range() {
    let all = RuntimeConfig::default();
    let config = RuntimeConfig::default()
        .with_action_phases([
            ActionPhase::new(i8::MIN),
            ActionPhase::new(0),
            ActionPhase::new(i8::MAX),
            ActionPhase::new(i8::MIN),
        ])
        .with_reaction_phases([ReactionPhase::new(i8::MIN), ReactionPhase::new(i8::MAX)]);
    let empty = RuntimeConfig::default()
        .with_action_phases([])
        .with_reaction_phases([]);

    assert_eq!(
        [
            all.action_phases.contains(i8::MIN),
            all.action_phases.contains(i8::MAX),
            config.action_phases.contains(i8::MIN),
            config.action_phases.contains(-1),
            config.action_phases.contains(0),
            config.action_phases.contains(1),
            config.action_phases.contains(i8::MAX),
            config.reaction_phases.contains(i8::MIN),
            config.reaction_phases.contains(0),
            config.reaction_phases.contains(i8::MAX),
            empty.action_phases.contains(0),
            empty.reaction_phases.contains(0),
        ],
        [
            true, true, true, false, true, false, true, true, false, true, false, false,
        ]
    );
}

#[test]
fn runtime_with_config_accepts_configured_phases() {
    let action_phase = ActionPhase::new(-1);
    let reaction_phase = ReactionPhase::new(2);
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
    let valid_action_phase = ActionPhase::new(1);
    let valid_reaction_phase = ReactionPhase::new(1);
    let mut rt = Runtime::new_with_config(
        RuntimeConfig::default()
            .with_action_phases([valid_action_phase])
            .with_reaction_phases([valid_reaction_phase]),
    );

    assert!(catch_unwind(|| Action::new(|_| {}).schedule_in(ActionPhase::new(2))).is_err());
    assert!(catch_unwind(|| Reaction::new(|_| {}).schedule_in(ReactionPhase::new(2))).is_err());
    assert!(catch_unwind(|| spawn_action_async_in(ActionPhase::new(2), async |_| {})).is_err());
    assert!(!rt.dispatch_all_actions());
    assert!(!rt.dispatch_all_reactions());
}

#[test]
fn runtime_with_config_rejects_invalid_dispatches_when_queues_are_empty() {
    let mut rt = Runtime::new_with_config(
        RuntimeConfig::default()
            .with_action_phases([ActionPhase::default()])
            .with_reaction_phases([ReactionPhase::default()]),
    );

    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            rt.dispatch_action(ActionPhase::new(1));
        }))
        .is_err()
    );
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            rt.dispatch_actions(ActionPhase::new(1));
        }))
        .is_err()
    );
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            rt.dispatch_reactions(ReactionPhase::new(1));
        }))
        .is_err()
    );
}

#[test]
fn runtime_with_config_accepts_valid_phases_scheduled_before_creation() {
    let action_phase = ActionPhase::new(1);
    let reaction_phase = ReactionPhase::new(2);
    let calls = Rc::new(RefCell::new(Vec::new()));
    schedule_call(action_phase, calls.clone(), 1);
    schedule_call(action_phase, calls.clone(), 2);
    schedule_reaction_call(reaction_phase, calls.clone(), 3);
    schedule_reaction_call(reaction_phase, calls.clone(), 4);

    let mut rt = Runtime::new_with_config(
        RuntimeConfig::default()
            .with_action_phases([action_phase])
            .with_reaction_phases([reaction_phase]),
    );

    assert!(rt.dispatch_actions(action_phase));
    assert!(rt.dispatch_reactions(reaction_phase));
    assert_eq!(*calls.borrow(), [1, 2, 3, 4]);
}

#[test]
fn runtime_creation_rejects_and_preserves_preexisting_invalid_action() {
    let phase = ActionPhase::new(1);
    Action::new(|_| {}).schedule_in(phase);

    assert!(
        catch_unwind(|| {
            Runtime::new_with_config(
                RuntimeConfig::default().with_action_phases([ActionPhase::default()]),
            );
        })
        .is_err()
    );

    let mut rt = Runtime::new();
    assert!(rt.dispatch_action(phase));
}

#[test]
fn runtime_creation_rejects_and_preserves_preexisting_invalid_reaction() {
    let phase = ReactionPhase::new(1);
    Reaction::new(|_| {}).schedule_in(phase);

    assert!(
        catch_unwind(|| {
            Runtime::new_with_config(
                RuntimeConfig::default().with_reaction_phases([ReactionPhase::default()]),
            );
        })
        .is_err()
    );

    let mut rt = Runtime::new();
    assert!(rt.dispatch_reactions(phase));
}

#[test]
fn runtime_config_remains_active_during_runtime_call() {
    let phase = ActionPhase::new(1);
    let mut rt = Runtime::new_with_config(RuntimeConfig::default().with_action_phases([phase]));
    {
        let _lend = rt.lend();
        Runtime::call(|_| Action::new(|_| {}).schedule_in(phase));
        assert!(
            catch_unwind(|| {
                Runtime::call(|_| Action::new(|_| {}).schedule_in(ActionPhase::default()));
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
    let phase = ActionPhase::new(1);
    let mut rt = Runtime::new_with_config(RuntimeConfig::default().with_action_phases([phase]));
    let (sender, receiver) = oneshot_broadcast();
    let called = Rc::new(Cell::new(false));
    spawn_action_async_in(phase, {
        let called = called.clone();
        async move |_| {
            receiver.recv().await;
            called.set(true);
        }
    });

    assert!(rt.dispatch_action(phase));
    assert!(!called.get());
    sender.send(());
    assert!(rt.dispatch_action(phase));
    assert!(called.get());
}

#[test]
fn runtime_creation_rejects_a_second_runtime_without_corrupting_the_first() {
    let phase = ActionPhase::new(1);
    let mut rt = Runtime::new_with_config(RuntimeConfig::default().with_action_phases([phase]));

    assert!(catch_unwind(Runtime::new).is_err());
    Action::new(|_| {}).schedule_in(phase);
    assert!(rt.dispatch_action(phase));
}

#[test]
fn dispatch_action_runs_one_action_in_fifo_order() {
    let mut rt = Runtime::new();
    let phase = ActionPhase::new(1);
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
    let mut rt = Runtime::new();
    let first_phase = ActionPhase::new(1);
    let second_phase = ActionPhase::new(2);
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
    let mut rt = Runtime::new();
    let phase = ActionPhase::new(1);
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
    let mut rt = Runtime::new();
    let phase = ActionPhase::new(1);
    let other_phase = ActionPhase::new(2);
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
    let mut rt = Runtime::new();
    let flag = Rc::new(Cell::new(false));
    let weak = Rc::downgrade(&flag);
    Action::from_weak_fn(weak, |flag, _| flag.set(true)).schedule();
    rt.dispatch_all_actions();
    assert!(flag.get());
}

#[test]
fn runtime_call_within_lend() {
    let mut rt = Runtime::new();
    let _lend = rt.lend();
    Runtime::call(|_rt| {});
}

#[test]
fn runtime_call_within_lend_2() {
    let mut rt = Runtime::new();
    let _lend = rt.lend();
    Runtime::call(|_rt| {});
    Runtime::call(|_rt| {});
}

#[test]
fn runtime_try_call_within_lend() {
    let mut rt = Runtime::new();
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
