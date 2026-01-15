use super::*;
use std::{cell::Cell, rc::Rc};

#[test]
fn task_kind_default_is_registered() {
    let _rt = Runtime::new();
    assert!(TaskKind::default().is_registered());
}

#[test]
fn action_kind_default_is_registered() {
    let _rt = Runtime::new();
    assert!(ActionKind::default().is_registered());
}

#[test]
fn task_kind_is_not_registered_before_register() {
    let _rt = Runtime::new();
    const KIND: TaskKind = TaskKind::new(10, "test");
    assert!(!KIND.is_registered());
}

#[test]
fn action_kind_is_not_registered_before_register() {
    let _rt = Runtime::new();
    const KIND: ActionKind = ActionKind::new(10, "test");
    assert!(!KIND.is_registered());
}

#[test]
fn task_kind_is_registered_after_register() {
    let _rt = Runtime::new();
    const KIND: TaskKind = TaskKind::new(1, "test");
    assert!(!KIND.is_registered());
    Runtime::register_task_kind(KIND);
    assert!(KIND.is_registered());
}

#[test]
fn action_kind_is_registered_after_register() {
    let _rt = Runtime::new();
    const KIND: ActionKind = ActionKind::new(1, "test");
    assert!(!KIND.is_registered());
    Runtime::register_action_kind(KIND);
    assert!(KIND.is_registered());
}

#[test]
fn task_kind_registration_cleared_after_runtime_drop() {
    const KIND: TaskKind = TaskKind::new(2, "test");
    {
        let _rt = Runtime::new();
        Runtime::register_task_kind(KIND);
        assert!(KIND.is_registered());
    }
    let _rt = Runtime::new();
    assert!(!KIND.is_registered());
}

#[test]
fn action_kind_registration_cleared_after_runtime_drop() {
    const KIND: ActionKind = ActionKind::new(2, "test");
    {
        let _rt = Runtime::new();
        Runtime::register_action_kind(KIND);
        assert!(KIND.is_registered());
    }
    let _rt = Runtime::new();
    assert!(!KIND.is_registered());
}

#[test]
#[should_panic(expected = "`TaskKind` 4: test is not registered.")]
fn schedule_task_with_unregistered_kind_panic() {
    let _rt = Runtime::new();
    const KIND: TaskKind = TaskKind::new(4, "test");
    Task::new(|_| {}).schedule_with(KIND);
}

#[test]
#[should_panic(expected = "`ActionKind` 4: test is not registered.")]
fn schedule_action_with_unregistered_kind_panic() {
    let _rt = Runtime::new();
    const KIND: ActionKind = ActionKind::new(4, "test");
    Action::new(|_| {}).schedule_with(KIND);
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
