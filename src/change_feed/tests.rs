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
        ChangeFeedDelta::Changes(changes) => Some(changes.copied().collect()),
    }
}

#[test]
fn reader_distinguishes_initial_and_incremental_changes() {
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
fn scan_records_changes_incrementally() {
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
