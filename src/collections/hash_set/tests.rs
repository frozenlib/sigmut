use super::*;
use crate::{State, core::Runtime, effect};
use pretty_assertions::assert_eq;
use std::{cell::Cell, rc::Rc};

#[test]
fn state_hash_set_reader_reports_insert_and_remove() {
    let mut rt = Runtime::new();
    let set = StateHashSet::new();
    let mut reader = set.reader();
    drop(reader.read(&mut rt.sc()));

    {
        let mut items = set.borrow_mut(rt.ac());
        assert!(items.insert(String::from("a")));
        assert!(items.insert(String::from("b")));
        assert!(!items.insert(String::from("a")));
        assert!(items.remove("b"));
        assert!(!items.remove("missing"));
    }

    let items = reader.read(&mut rt.sc());
    assert!(items.contains("a"));
    assert!(!items.contains("b"));
    assert_eq!(
        items.delta().collect::<Vec<_>>(),
        [
            HashSetChange::Insert {
                new_value: &String::from("a"),
            },
            HashSetChange::Insert {
                new_value: &String::from("b"),
            },
            HashSetChange::Remove {
                old_value: &String::from("b"),
            },
        ]
    );
}

#[test]
fn state_hash_set_refs_basic_apis() {
    let mut rt = Runtime::new();
    let set = StateHashSet::<String>::default();
    {
        let items = set.borrow(&mut rt.sc());
        assert_eq!((items.len(), items.is_empty()), (0, true));
        assert_eq!(format!("{items:?}"), "{}");
    }
    {
        let mut items = set.borrow_mut(rt.ac());
        assert_eq!((items.len(), items.is_empty()), (0, true));
        items.insert(String::from("a"));
        items.insert(String::from("b"));
        assert!(items.contains("a"));
        assert_eq!(items.iter().count(), 2);
        assert_eq!((&items).into_iter().count(), 2);
        assert!(format!("{items:?}").contains("\"a\""));
        items.clear();
        assert_eq!((items.len(), items.is_empty()), (0, true));
    }
    let items = set.borrow(&mut rt.sc());
    assert_eq!(
        (&items).into_iter().collect::<Vec<_>>(),
        Vec::<&String>::new()
    );
}

#[test]
fn state_backed_signal_hash_set_exposes_collection_reads() {
    let mut rt = Runtime::new();
    let set = StateHashSet::new();
    set.borrow_mut(rt.ac()).insert(String::from("a"));
    let signal = set.to_signal();

    let items = signal.borrow(&mut rt.sc());
    assert_eq!((items.len(), items.contains("a")), (1, true));
    assert_eq!((&items).into_iter().count(), 1);
    assert_eq!(format!("{items:?}"), "{\"a\"}");
}

#[test]
fn state_hash_set_duplicate_insert_and_missing_remove_do_not_notify() {
    let mut rt = Runtime::new();
    let set = StateHashSet::new();
    set.borrow_mut(rt.ac()).insert(String::from("a"));
    let calls = Rc::new(Cell::new(0));
    let _subscription = effect({
        let calls = calls.clone();
        let set = set.clone();
        move |sc| {
            drop(set.borrow(sc));
            calls.set(calls.get() + 1);
        }
    });
    rt.flush();

    assert!(!set.borrow_mut(rt.ac()).insert(String::from("a")));
    assert!(!set.borrow_mut(rt.ac()).remove("missing"));
    rt.flush();

    assert_eq!(calls.get(), 1);
}

#[test]
fn state_hash_set_tracks_the_collection_as_one_state() {
    let mut rt = Runtime::new();
    let set = StateHashSet::new();
    set.borrow_mut(rt.ac()).insert(String::from("a"));
    let calls = Rc::new(Cell::new(0));
    let _subscription = effect({
        let calls = calls.clone();
        let set = set.clone();
        move |sc| {
            assert!(set.borrow(sc).contains("a"));
            calls.set(calls.get() + 1);
        }
    });
    rt.flush();

    set.borrow_mut(rt.ac()).insert(String::from("b"));
    rt.flush();

    assert_eq!(calls.get(), 2);
}

#[test]
fn state_hash_set_loose_borrow_notifies_the_collection() {
    let mut rt = Runtime::new();
    let set = StateHashSet::<String>::new();
    let calls = Rc::new(Cell::new(0));
    let _subscription = effect({
        let calls = calls.clone();
        let set = set.clone();
        move |sc| {
            let contains = set.borrow(sc).contains("b");
            assert_eq!(contains, calls.get() != 0);
            calls.set(calls.get() + 1);
        }
    });
    rt.flush();

    set.borrow_mut_loose(rt.ac()).insert(String::from("b"));
    rt.flush();

    assert_eq!(calls.get(), 2);
}

#[test]
fn signal_hash_set_scan_and_reader_share_hash_map_history() {
    let mut rt = Runtime::new();
    let enabled = State::new(false);
    let signal = SignalHashSet::from_scan({
        let enabled = enabled.clone();
        move |items, sc| {
            if enabled.get(sc) {
                items.insert(String::from("a"));
            } else {
                items.remove("a");
            }
        }
    });
    let mut reader = signal.reader();
    drop(reader.read(&mut rt.sc()));

    enabled.set(true, rt.ac());
    let items = reader.read(&mut rt.sc());
    assert!(items.contains("a"));
    assert_eq!(
        items.delta().collect::<Vec<_>>(),
        [HashSetChange::Insert {
            new_value: &String::from("a"),
        }]
    );
}

#[test]
fn signal_hash_set_reader_peek_and_clone_have_independent_cursors() {
    let mut rt = Runtime::new();
    let enabled = State::new(false);
    let signal = SignalHashSet::from_scan_with_hasher(RandomState::new(), {
        let enabled = enabled.clone();
        move |items, sc| {
            if enabled.get(sc) {
                items.insert(String::from("a"));
            } else {
                items.remove("a");
            }
        }
    });
    let mut reader = signal.reader();
    drop(reader.read(&mut rt.sc()));
    let mut reader_clone = reader.clone();

    enabled.set(true, rt.ac());
    for _ in 0..2 {
        assert_eq!(
            reader.peek(&mut rt.sc()).delta().collect::<Vec<_>>(),
            [HashSetChange::Insert {
                new_value: &String::from("a"),
            }]
        );
    }
    assert_eq!(reader.read(&mut rt.sc()).delta().count(), 1);
    assert_eq!(reader_clone.read(&mut rt.sc()).delta().count(), 1);
}

#[test]
fn state_hash_set_serde_preserves_values_and_starts_reader_as_initial() {
    let set: StateHashSet<String> = [String::from("a"), String::from("b")].into_iter().collect();
    let json = serde_json::to_string(&set).unwrap();
    let restored: StateHashSet<String> = serde_json::from_str(&json).unwrap();
    let mut rt = Runtime::new();
    let mut reader = restored.reader();
    let items = reader.read(&mut rt.sc());

    assert!(items.contains("a"));
    assert!(items.contains("b"));
    assert_eq!(items.delta().count(), 2);
}

#[test]
fn state_hash_set_serialization_reports_borrow_conflict() {
    let mut rt = Runtime::new();
    let set = StateHashSet::<String>::new();
    let _items = set.borrow_mut(rt.ac());

    assert!(serde_json::to_string(&set).is_err());
}
