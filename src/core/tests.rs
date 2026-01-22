use super::*;
use std::{cell::Cell, rc::Rc};

#[test]
fn reaction_phase_default_is_registered() {
    let _rt = Runtime::new();
    assert!(ReactionPhase::default().is_registered());
}

#[test]
fn action_phase_default_is_registered() {
    let _rt = Runtime::new();
    assert!(ActionPhase::default().is_registered());
}

#[test]
fn reaction_phase_is_not_registered_before_register() {
    let _rt = Runtime::new();
    const PHASE: ReactionPhase = ReactionPhase::new(10, "test");
    assert!(!PHASE.is_registered());
}

#[test]
fn action_phase_is_not_registered_before_register() {
    let _rt = Runtime::new();
    const PHASE: ActionPhase = ActionPhase::new(10, "test");
    assert!(!PHASE.is_registered());
}

#[test]
fn reaction_phase_is_registered_after_register() {
    let _rt = Runtime::new();
    const PHASE: ReactionPhase = ReactionPhase::new(1, "test");
    assert!(!PHASE.is_registered());
    Runtime::register_reaction_phase(PHASE);
    assert!(PHASE.is_registered());
}

#[test]
fn action_phase_is_registered_after_register() {
    let _rt = Runtime::new();
    const PHASE: ActionPhase = ActionPhase::new(1, "test");
    assert!(!PHASE.is_registered());
    Runtime::register_action_phase(PHASE);
    assert!(PHASE.is_registered());
}

#[test]
fn reaction_phase_registration_cleared_after_runtime_drop() {
    const PHASE: ReactionPhase = ReactionPhase::new(2, "test");
    {
        let _rt = Runtime::new();
        Runtime::register_reaction_phase(PHASE);
        assert!(PHASE.is_registered());
    }
    let _rt = Runtime::new();
    assert!(!PHASE.is_registered());
}

#[test]
fn action_phase_registration_cleared_after_runtime_drop() {
    const PHASE: ActionPhase = ActionPhase::new(2, "test");
    {
        let _rt = Runtime::new();
        Runtime::register_action_phase(PHASE);
        assert!(PHASE.is_registered());
    }
    let _rt = Runtime::new();
    assert!(!PHASE.is_registered());
}

#[test]
#[should_panic(expected = "`ReactionPhase` 4: test is not registered.")]
fn schedule_reaction_with_unregistered_phase_panic() {
    let _rt = Runtime::new();
    const PHASE: ReactionPhase = ReactionPhase::new(4, "test");
    Reaction::new(|_| {}).schedule_with(PHASE);
}

#[test]
#[should_panic(expected = "`ActionPhase` 4: test is not registered.")]
fn schedule_action_with_unregistered_phase_panic() {
    let _rt = Runtime::new();
    const PHASE: ActionPhase = ActionPhase::new(4, "test");
    Action::new(|_| {}).schedule_with(PHASE);
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
#[should_panic(expected = "Runtime is not available")]
fn runtime_call_outside_lend() {
    let _rt = Runtime::new();
    Runtime::call(|_rt| {});
}
