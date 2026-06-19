use super::*;
use pretty_assertions::assert_eq;
use std::{cell::Cell, rc::Rc};

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
