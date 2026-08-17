use super::*;
use crate::{State, core::Runtime, effect};
use pretty_assertions::assert_eq;
use std::{
    cell::Cell,
    hash::{BuildHasher, Hasher},
    rc::Rc,
};

#[test]
fn state_hash_map_reader_reports_insert_set_and_remove_in_order() {
    let mut rt = Runtime::new();
    let map = StateHashMap::new();
    let mut reader = map.reader();

    assert_eq!(reader.read(&mut rt.sc()).delta().collect::<Vec<_>>(), []);
    {
        let mut items = map.borrow_mut(rt.ac());
        assert!(items.insert(String::from("a"), 1));
        assert!(items.insert(String::from("b"), 2));
        assert!(!items.insert(String::from("a"), 3));
        assert!(items.remove("b"));
        assert!(!items.remove("missing"));
    }

    let items = reader.read(&mut rt.sc());
    assert_eq!(items.get("a"), Some(&3));
    assert_eq!(items.get("b"), None);
    assert_eq!(
        items.delta().collect::<Vec<_>>(),
        [
            HashMapChange::Insert {
                key: &String::from("a"),
                new_value: &1,
            },
            HashMapChange::Insert {
                key: &String::from("b"),
                new_value: &2,
            },
            HashMapChange::Set {
                key: &String::from("a"),
                old_value: &1,
                new_value: &3,
            },
            HashMapChange::Remove {
                key: &String::from("b"),
                old_value: &2,
            },
        ]
    );
}

#[test]
fn state_hash_map_items_and_mutable_items_basic_apis() {
    let mut rt = Runtime::new();
    let map = StateHashMap::<String, i32>::default();
    {
        let items = map.items(&mut rt.sc());
        assert_eq!((items.len(), items.is_empty()), (0, true));
        assert_eq!(format!("{items:?}"), "{}");
    }
    {
        let mut items = map.borrow_mut(rt.ac());
        assert_eq!((items.len(), items.is_empty()), (0, true));
        items.insert(String::from("a"), 1);
        items.insert(String::from("b"), 2);
        assert_eq!(items.get("a"), Some(&1));
        assert!(items.contains_key("b"));
        assert_eq!(items.iter().count(), 2);
        assert_eq!((&items).into_iter().count(), 2);
        assert!(format!("{items:?}").contains("\"a\": 1"));
        items.clear();
        assert_eq!((items.len(), items.is_empty()), (0, true));
    }
    let items = map.items(&mut rt.sc());
    assert_eq!(
        (&items).into_iter().collect::<Vec<_>>(),
        Vec::<(&String, &i32)>::new()
    );
}

#[test]
fn state_backed_signal_hash_map_exposes_key_and_collection_reads() {
    let mut rt = Runtime::new();
    let map = StateHashMap::new();
    map.insert(String::from("a"), 1, rt.ac());
    let signal = map.to_signal_hash_map();

    assert_eq!(signal.get("a", &mut rt.sc()).as_deref(), Some(&1));
    let items = signal.items(&mut rt.sc());
    assert_eq!((items.len(), items.contains_key("a")), (1, true));
    assert_eq!((&items).into_iter().count(), 1);
    assert_eq!(format!("{items:?}"), "{\"a\": 1}");
}

#[test]
fn state_hash_map_reader_peek_and_clone_have_independent_cursors() {
    let mut rt = Runtime::new();
    let map = StateHashMap::new();
    let mut reader = map.reader();
    drop(reader.read(&mut rt.sc()));
    let mut reader_clone = reader.clone();

    map.insert("a", 1, rt.ac());
    for _ in 0..2 {
        assert_eq!(
            reader.peek(&mut rt.sc()).delta().collect::<Vec<_>>(),
            [HashMapChange::Insert {
                key: &"a",
                new_value: &1,
            }]
        );
    }
    assert_eq!(
        reader.read(&mut rt.sc()).delta().collect::<Vec<_>>(),
        [HashMapChange::Insert {
            key: &"a",
            new_value: &1,
        }]
    );
    assert_eq!(
        reader_clone.read(&mut rt.sc()).delta().collect::<Vec<_>>(),
        [HashMapChange::Insert {
            key: &"a",
            new_value: &1,
        }]
    );
}

#[test]
fn state_hash_map_retains_removed_key_and_value_until_reader_advances() {
    let mut rt = Runtime::new();
    let map = StateHashMap::new();
    let key = Rc::new(String::from("key"));
    let value = Rc::new(String::from("value"));
    map.insert(key.clone(), value.clone(), rt.ac());
    let mut reader = map.reader();
    drop(reader.read(&mut rt.sc()));

    map.remove(&key, rt.ac());
    assert_eq!((Rc::strong_count(&key), Rc::strong_count(&value)), (2, 2));

    let items = reader.read(&mut rt.sc());
    assert_eq!(
        items.delta().collect::<Vec<_>>(),
        [HashMapChange::Remove {
            key: &key,
            old_value: &value,
        }]
    );
    drop(items);

    assert_eq!((Rc::strong_count(&key), Rc::strong_count(&value)), (1, 1));
}

#[test]
fn state_hash_map_can_reinsert_an_equal_key_while_removed_history_is_retained() {
    let mut rt = Runtime::new();
    let map = StateHashMap::new();
    map.insert(String::from("key"), 1, rt.ac());
    let mut reader = map.reader();
    drop(reader.read(&mut rt.sc()));

    map.remove("key", rt.ac());
    map.insert(String::from("key"), 2, rt.ac());
    let items = reader.read(&mut rt.sc());

    assert_eq!(items.get("key"), Some(&2));
    assert_eq!(
        items.delta().collect::<Vec<_>>(),
        [
            HashMapChange::Remove {
                key: &String::from("key"),
                old_value: &1,
            },
            HashMapChange::Insert {
                key: &String::from("key"),
                new_value: &2,
            },
        ]
    );
}

#[test]
fn state_hash_map_releases_replaced_and_removed_values_without_readers() {
    let mut rt = Runtime::new();
    let map = StateHashMap::new();
    let old = Rc::new(String::from("old"));
    let new = Rc::new(String::from("new"));
    map.insert("key", old.clone(), rt.ac());
    map.insert("key", new.clone(), rt.ac());
    assert_eq!(Rc::strong_count(&old), 1);

    map.remove("key", rt.ac());
    assert_eq!(Rc::strong_count(&new), 1);
}

#[test]
fn state_hash_map_tracks_borrowed_key_missing_key_and_all_items_separately() {
    let mut rt = Runtime::new();
    let map = StateHashMap::new();
    map.insert(String::from("a"), 1, rt.ac());
    map.insert(String::from("b"), 2, rt.ac());

    let a_calls = Rc::new(Cell::new(0));
    let _a_subscription = effect({
        let calls = a_calls.clone();
        let map = map.clone();
        move |sc| {
            assert_eq!(map.get("a", sc).as_deref(), Some(&1));
            calls.set(calls.get() + 1);
        }
    });
    let missing_calls = Rc::new(Cell::new(0));
    let _missing_subscription = effect({
        let calls = missing_calls.clone();
        let map = map.clone();
        move |sc| {
            drop(map.get("missing", sc));
            calls.set(calls.get() + 1);
        }
    });
    let items_calls = Rc::new(Cell::new(0));
    let _items_subscription = effect({
        let calls = items_calls.clone();
        let map = map.clone();
        move |sc| {
            drop(map.items(sc));
            calls.set(calls.get() + 1);
        }
    });
    rt.flush();
    assert_eq!(
        (a_calls.get(), missing_calls.get(), items_calls.get()),
        (1, 1, 1)
    );

    map.insert(String::from("b"), 20, rt.ac());
    rt.flush();
    assert_eq!(
        (a_calls.get(), missing_calls.get(), items_calls.get()),
        (1, 1, 2)
    );

    map.insert(String::from("missing"), 3, rt.ac());
    rt.flush();
    assert_eq!(
        (a_calls.get(), missing_calls.get(), items_calls.get()),
        (1, 2, 3)
    );
}

#[test]
fn state_hash_map_empty_batch_does_not_notify() {
    let mut rt = Runtime::new();
    let map = StateHashMap::<i32, i32>::new();
    let calls = Rc::new(Cell::new(0));
    let _subscription = effect({
        let calls = calls.clone();
        let map = map.clone();
        move |sc| {
            drop(map.items(sc));
            calls.set(calls.get() + 1);
        }
    });
    rt.flush();
    drop(map.borrow_mut(rt.ac()));
    rt.flush();

    assert_eq!(calls.get(), 1);
}

#[test]
fn signal_hash_map_scan_cleans_unchanged_and_unrelated_key_dependencies() {
    let mut rt = Runtime::new();
    let count = State::new(1);
    let signal = SignalHashMap::from_scan({
        let count = count.clone();
        move |items, sc| match count.get(sc) {
            0 => {
                items.remove("a");
            }
            1 => {
                if !items.contains_key("a") {
                    items.insert(String::from("a"), 1);
                }
            }
            _ => {
                if !items.contains_key("b") {
                    items.insert(String::from("b"), 2);
                }
            }
        }
    });
    let calls = Rc::new(Cell::new(0));
    let _subscription = effect({
        let calls = calls.clone();
        let signal = signal.clone();
        move |sc| {
            assert_eq!(signal.get("a", sc).as_deref(), Some(&1));
            calls.set(calls.get() + 1);
        }
    });
    rt.flush();
    assert_eq!(calls.get(), 1);

    count.set(1, rt.ac());
    rt.flush();
    assert_eq!(calls.get(), 1);

    count.set(2, rt.ac());
    rt.flush();
    assert_eq!(calls.get(), 1);
}

#[test]
fn signal_hash_map_custom_hasher_reader_peek_and_clone_are_supported() {
    let mut rt = Runtime::new();
    let enabled = State::new(false);
    let signal = SignalHashMap::from_scan_with_hasher(ZeroBuildHasher, {
        let enabled = enabled.clone();
        move |items, sc| {
            if enabled.get(sc) {
                items.insert(String::from("a"), 1);
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
            [HashMapChange::Insert {
                key: &String::from("a"),
                new_value: &1,
            }]
        );
    }
    assert_eq!(reader.read(&mut rt.sc()).delta().count(), 1);
    assert_eq!(reader_clone.read(&mut rt.sc()).delta().count(), 1);
}

#[derive(Clone, Copy, Default)]
struct ZeroBuildHasher;

struct ZeroHasher;

impl Hasher for ZeroHasher {
    fn finish(&self) -> u64 {
        0
    }

    fn write(&mut self, _bytes: &[u8]) {}
}

impl BuildHasher for ZeroBuildHasher {
    type Hasher = ZeroHasher;

    fn build_hasher(&self) -> Self::Hasher {
        ZeroHasher
    }
}

#[test]
fn hash_collisions_keep_distinct_entries_correct() {
    let mut rt = Runtime::new();
    let map = StateHashMap::with_hasher(ZeroBuildHasher);
    map.insert(String::from("a"), 1, rt.ac());
    map.insert(String::from("b"), 2, rt.ac());

    assert_eq!(map.get("a", &mut rt.sc()).as_deref(), Some(&1));
    assert_eq!(map.get("b", &mut rt.sc()).as_deref(), Some(&2));
    assert!(map.remove("a", rt.ac()));
    assert_eq!(map.get("a", &mut rt.sc()).as_deref(), None);
    assert_eq!(map.get("b", &mut rt.sc()).as_deref(), Some(&2));
}

#[test]
fn state_hash_map_serde_preserves_entries_and_starts_reader_as_initial() {
    let map: StateHashMap<String, i32> = [(String::from("a"), 1), (String::from("b"), 2)]
        .into_iter()
        .collect();
    let json = serde_json::to_string(&map).unwrap();
    let restored: StateHashMap<String, i32> = serde_json::from_str(&json).unwrap();
    let mut rt = Runtime::new();
    let mut reader = restored.reader();
    let items = reader.read(&mut rt.sc());

    assert_eq!((items.get("a"), items.get("b")), (Some(&1), Some(&2)));
    assert_eq!(items.delta().count(), 2);
}

#[test]
fn state_hash_map_serialization_reports_borrow_conflict() {
    let mut rt = Runtime::new();
    let map = StateHashMap::<String, i32>::new();
    let _items = map.borrow_mut(rt.ac());

    assert!(serde_json::to_string(&map).is_err());
}

#[test]
fn reserved_items_slot_hash_is_remapped() {
    assert_ne!(hash_to_slot(u64::MAX), SLOT_ITEMS);
}
