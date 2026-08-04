use std::{cell::RefCell, rc::Rc};

use pretty_assertions::assert_eq;

use super::*;
use crate::{State, core::Runtime};

struct TestModel {
    value: i32,
    released: Rc<RefCell<Vec<i32>>>,
}

impl ChangeFeedModel for TestModel {
    type Change = i32;

    fn release_change(&mut self, change: Self::Change) {
        self.released.borrow_mut().push(change);
    }
}

fn set(edit: &mut ChangeFeedRefMut<'_, TestModel>, value: i32) {
    let old = edit.current().value;
    edit.current_mut().value = value;
    edit.record(old);
}

fn delta(value: &ChangeFeedRef<'_, TestModel>) -> Option<Vec<i32>> {
    match value.delta() {
        ChangeFeedDelta::Initial => None,
        ChangeFeedDelta::Incremental(changes) => Some(changes.copied().collect()),
    }
}

#[test]
fn edit_changes_are_empty_without_records() {
    let mut rt = Runtime::new();
    let state = ChangeFeedState::new(TestModel {
        value: 0,
        released: Rc::new(RefCell::new(Vec::new())),
    });
    let edit = state.borrow_mut(rt.ac());

    assert_eq!(
        edit.changes().copied().collect::<Vec<_>>(),
        Vec::<i32>::new()
    );
}

#[test]
fn edit_changes_are_ordered_and_exclude_changes_before_the_edit() {
    let mut rt = Runtime::new();
    let state = ChangeFeedState::new(TestModel {
        value: 0,
        released: Rc::new(RefCell::new(Vec::new())),
    });
    let mut reader = state.reader();
    drop(reader.read(&mut rt.sc()));
    set(&mut state.borrow_mut(rt.ac()), 1);

    let mut edit = state.borrow_mut(rt.ac());
    set(&mut edit, 2);
    set(&mut edit, 3);

    assert_eq!(
        (
            edit.current().value,
            edit.changes().copied().collect::<Vec<_>>()
        ),
        (3, vec![1, 2]),
    );
}

#[test]
fn reader_distinguishes_initial_and_incremental_deltas() {
    let mut rt = Runtime::new();
    let state = ChangeFeedState::new(TestModel {
        value: 0,
        released: Rc::new(RefCell::new(Vec::new())),
    });
    let mut reader = state.reader();

    assert_eq!(delta(&reader.peek(&mut rt.sc())), None);
    assert_eq!(delta(&reader.read(&mut rt.sc())), None);
    assert_eq!(delta(&reader.peek(&mut rt.sc())), Some(Vec::new()));

    set(&mut state.borrow_mut(rt.ac()), 1);

    assert_eq!(delta(&reader.read(&mut rt.sc())), Some(vec![0]));
    assert_eq!(delta(&reader.peek(&mut rt.sc())), Some(Vec::new()));
}

#[test]
fn cloned_readers_retain_independent_cursors() {
    let mut rt = Runtime::new();
    let released = Rc::new(RefCell::new(Vec::new()));
    let state = ChangeFeedState::new(TestModel {
        value: 0,
        released: released.clone(),
    });
    let mut reader = state.reader();
    drop(reader.read(&mut rt.sc()));
    let mut reader_clone = reader.clone();

    set(&mut state.borrow_mut(rt.ac()), 1);
    assert_eq!(delta(&reader.read(&mut rt.sc())), Some(vec![0]));
    assert_eq!(*released.borrow(), Vec::<i32>::new());

    set(&mut state.borrow_mut(rt.ac()), 2);
    assert_eq!(delta(&reader.read(&mut rt.sc())), Some(vec![1]));
    assert_eq!(delta(&reader_clone.read(&mut rt.sc())), Some(vec![0, 1]));
    assert_eq!(&*released.borrow(), &[0, 1]);
}

#[test]
fn ref_drop_applies_deferred_reader_advance() {
    let mut rt = Runtime::new();
    let released = Rc::new(RefCell::new(Vec::new()));
    let state = ChangeFeedState::new(TestModel {
        value: 0,
        released: released.clone(),
    });
    let mut reader = state.reader();
    drop(reader.read(&mut rt.sc()));
    set(&mut state.borrow_mut(rt.ac()), 1);

    let value = reader.read(&mut rt.sc());
    assert_eq!(delta(&value), Some(vec![0]));
    assert_eq!(*released.borrow(), Vec::<i32>::new());
    drop(value);

    assert_eq!(&*released.borrow(), &[0]);
}

#[test]
fn contextless_borrow_returns_materialized_current_without_advancing_reader() {
    let mut rt = Runtime::new();
    let state = ChangeFeedState::new(TestModel {
        value: 1,
        released: Rc::new(RefCell::new(Vec::new())),
    });
    let mut reader = state.reader();

    assert_eq!(state.try_borrow_contextless().unwrap().current().value, 1);
    assert_eq!(delta(&reader.read(&mut rt.sc())), None);
}

#[test]
fn contextless_borrow_returns_error_while_mutably_borrowed() {
    let mut rt = Runtime::new();
    let state = ChangeFeedState::new(TestModel {
        value: 1,
        released: Rc::new(RefCell::new(Vec::new())),
    });
    let _edit = state.borrow_mut(rt.ac());

    assert!(state.try_borrow_contextless().is_err());
}

#[test]
fn scan_reports_incremental_deltas() {
    let mut rt = Runtime::new();
    let source = State::new(1);
    let signal = ChangeFeedSignal::from_scan(
        TestModel {
            value: 0,
            released: Rc::new(RefCell::new(Vec::new())),
        },
        {
            let source = source.clone();
            move |mut edit, sc| {
                let value = source.get(sc);
                if edit.current().value != value {
                    set(&mut edit, value);
                }
            }
        },
    );
    let mut reader = signal.reader();

    assert_eq!(delta(&reader.read(&mut rt.sc())), None);
    assert_eq!(signal.borrow(&mut rt.sc()).current().value, 1);

    source.set(2, rt.ac());
    assert_eq!(delta(&reader.read(&mut rt.sc())), Some(vec![1]));
    assert_eq!(signal.borrow(&mut rt.sc()).current().value, 2);
}
