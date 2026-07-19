use super::*;
use pretty_assertions::assert_eq;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn schedule_call(phase: ActionPhase, calls: Rc<RefCell<Vec<i32>>>, value: i32) {
    Action::new(move |_| calls.borrow_mut().push(value)).schedule_in(phase);
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
