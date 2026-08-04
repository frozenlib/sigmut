use super::*;
use crate::{State, core::Runtime, effect};
use pretty_assertions::assert_eq;
use std::{cell::Cell, rc::Rc};

#[test]
fn state_slab_map_reader_delta() {
    let mut rt = Runtime::new();
    let map = StateSlabMap::new();
    let mut reader = map.reader();

    {
        let items0 = reader.read(&mut rt.sc());
        assert!(items0.is_empty());
    }

    let key1 = map.insert(10, rt.ac());
    let key2 = map.insert(20, rt.ac());

    {
        let items1 = reader.read(&mut rt.sc());
        let changes: Vec<_> = items1.delta().collect();
        let expected = vec![
            SlabMapChange::Insert {
                key: key1,
                new_value: &10,
            },
            SlabMapChange::Insert {
                key: key2,
                new_value: &20,
            },
        ];
        assert_eq!(changes, expected);
    }

    map.remove(key1, rt.ac());
    {
        let items2 = reader.read(&mut rt.sc());
        let changes: Vec<_> = items2.delta().collect();
        let expected = vec![SlabMapChange::Remove {
            key: key1,
            old_value: &10,
        }];
        assert_eq!(changes, expected);
    }
}

#[test]
fn signal_slab_map_from_scan_updates() {
    let mut rt = Runtime::new();
    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
    enum MapAction {
        Insert,
        Remove,
    }

    let state = State::new(MapAction::Remove);
    let state_for_scan = state.clone();
    let key = Rc::new(Cell::new(None));

    let signal = SignalSlabMap::from_scan({
        let key = key.clone();
        move |items, sc| {
            let value = state_for_scan.get(sc);
            match (value, key.get()) {
                (MapAction::Remove, Some(k)) => {
                    items.remove(k);
                    key.set(None);
                }
                (MapAction::Insert, None) => {
                    let k = items.insert(1);
                    key.set(Some(k));
                }
                _ => {}
            }
        }
    });

    {
        let items0 = signal.items(&mut rt.sc());
        assert!(items0.is_empty());
    }

    state.set(MapAction::Insert, rt.ac());
    {
        let items1 = signal.items(&mut rt.sc());
        let values: Vec<_> = items1.iter().map(|(_, value)| *value).collect();
        assert_eq!(values, vec![1]);
    }

    state.set(MapAction::Remove, rt.ac());
    {
        let items2 = signal.items(&mut rt.sc());
        assert!(items2.is_empty());
    }
}

#[test]
fn state_slab_map_reader_peek_and_clone_have_independent_cursors() {
    let mut rt = Runtime::new();
    let map = StateSlabMap::new();
    let mut reader = map.reader();
    drop(reader.read(&mut rt.sc()));
    let mut reader_clone = reader.clone();

    let key = map.insert(10, rt.ac());
    for _ in 0..2 {
        let items = reader.peek(&mut rt.sc());
        assert_eq!(
            items.delta().collect::<Vec<_>>(),
            [SlabMapChange::Insert {
                key,
                new_value: &10,
            }]
        );
    }

    assert_eq!(
        reader.read(&mut rt.sc()).delta().collect::<Vec<_>>(),
        [SlabMapChange::Insert {
            key,
            new_value: &10,
        }]
    );
    assert_eq!(
        reader_clone.read(&mut rt.sc()).delta().collect::<Vec<_>>(),
        [SlabMapChange::Insert {
            key,
            new_value: &10,
        }]
    );
}

#[test]
fn state_slab_map_releases_removed_values_after_the_last_reader_advances() {
    let mut rt = Runtime::new();
    let map = StateSlabMap::new();
    let old = Rc::new(String::from("old"));
    let key = map.insert(old.clone(), rt.ac());
    let mut reader = map.reader();
    drop(reader.read(&mut rt.sc()));

    map.remove(key, rt.ac());
    assert_eq!(Rc::strong_count(&old), 2);

    let items = reader.read(&mut rt.sc());
    assert_eq!(
        items.delta().collect::<Vec<_>>(),
        [SlabMapChange::Remove {
            key,
            old_value: &old,
        }]
    );
    assert_eq!(Rc::strong_count(&old), 2);
    drop(items);

    assert_eq!(Rc::strong_count(&old), 1);
}

#[test]
fn state_slab_map_releases_removed_values_without_readers() {
    let mut rt = Runtime::new();
    let map = StateSlabMap::new();
    let old = Rc::new(String::from("old"));
    let key = map.insert(old.clone(), rt.ac());

    map.remove(key, rt.ac());

    assert_eq!(Rc::strong_count(&old), 1);
}

#[test]
fn state_slab_map_retries_compaction_after_an_item_borrow() {
    let mut rt = Runtime::new();
    let map = StateSlabMap::new();
    let retained_key = map.insert(Rc::new(String::from("retained")), rt.ac());
    let old = Rc::new(String::from("old"));
    let removed_key = map.insert(old.clone(), rt.ac());
    let mut reader = map.reader();
    drop(reader.read(&mut rt.sc()));
    map.remove(removed_key, rt.ac());

    let retained = map.0.item(retained_key);
    let items = reader.read(&mut rt.sc());
    assert_eq!(
        items.delta().collect::<Vec<_>>(),
        [SlabMapChange::Remove {
            key: removed_key,
            old_value: &old,
        }]
    );
    drop(items);
    assert_eq!(Rc::strong_count(&old), 2);

    drop(retained);
    assert_eq!(Rc::strong_count(&old), 2);
    drop(map.items(&mut rt.sc()));

    assert_eq!(Rc::strong_count(&old), 1);
}

#[test]
fn state_slab_map_tracks_item_and_all_items_separately() {
    let mut rt = Runtime::new();
    let map = StateSlabMap::new();
    let key0 = map.insert(10, rt.ac());
    let key1 = map.insert(20, rt.ac());
    assert_eq!((key0, key1), (0, 1));

    let item_calls = Rc::new(Cell::new(0));
    let _item_subscription = effect({
        let calls = item_calls.clone();
        let map = map.clone();
        move |sc| {
            drop(map.item(key0, sc));
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
    assert_eq!((item_calls.get(), items_calls.get()), (1, 1));

    map.remove(key1, rt.ac());
    rt.flush();

    assert_eq!((item_calls.get(), items_calls.get()), (1, 2));
}

#[test]
fn signal_slab_map_scan_does_not_propagate_unchanged_updates() {
    let mut rt = Runtime::new();
    let enabled = State::new(false);
    let key = Rc::new(Cell::new(None));
    let signal = SignalSlabMap::from_scan({
        let enabled = enabled.clone();
        let key = key.clone();
        move |items, sc| match (enabled.get(sc), key.get()) {
            (true, None) => key.set(Some(items.insert(1))),
            (false, Some(item_key)) => {
                items.remove(item_key);
                key.set(None);
            }
            _ => {}
        }
    });
    let calls = Rc::new(Cell::new(0));
    let _subscription = effect({
        let calls = calls.clone();
        move |sc| {
            drop(signal.items(sc));
            calls.set(calls.get() + 1);
        }
    });
    rt.flush();
    assert_eq!(calls.get(), 1);

    enabled.set(false, rt.ac());
    rt.flush();
    assert_eq!(calls.get(), 1);

    enabled.set(true, rt.ac());
    rt.flush();
    assert_eq!(calls.get(), 2);
}

#[test]
fn signal_slab_map_scan_cleans_unchanged_item_dependencies() {
    let mut rt = Runtime::new();
    let count = State::new(1_usize);
    let signal = SignalSlabMap::from_scan({
        let count = count.clone();
        move |items, sc| {
            while items.len < count.get(sc) {
                items.insert(items.len);
            }
        }
    });
    let calls = Rc::new(Cell::new(0));
    let _subscription = effect({
        let calls = calls.clone();
        move |sc| {
            drop(signal.item(0, sc));
            calls.set(calls.get() + 1);
        }
    });
    rt.flush();
    assert_eq!(calls.get(), 1);

    count.set(2, rt.ac());
    rt.flush();

    assert_eq!(calls.get(), 1);
}

#[test]
fn signal_slab_map_scan_readers_share_history_with_independent_cursors() {
    let mut rt = Runtime::new();
    let enabled = State::new(false);
    let key = Rc::new(Cell::new(None));
    let signal = SignalSlabMap::from_scan({
        let enabled = enabled.clone();
        let key = key.clone();
        move |items, sc| match (enabled.get(sc), key.get()) {
            (true, None) => key.set(Some(items.insert(1))),
            (false, Some(item_key)) => {
                items.remove(item_key);
                key.set(None);
            }
            _ => {}
        }
    });
    let mut reader = signal.reader();
    drop(reader.read(&mut rt.sc()));
    let mut reader_clone = reader.clone();

    enabled.set(true, rt.ac());
    let key = key.get().unwrap_or(0);
    for _ in 0..2 {
        assert_eq!(
            reader.peek(&mut rt.sc()).delta().collect::<Vec<_>>(),
            [SlabMapChange::Insert { key, new_value: &1 }]
        );
    }
    assert_eq!(
        reader.read(&mut rt.sc()).delta().collect::<Vec<_>>(),
        [SlabMapChange::Insert { key, new_value: &1 }]
    );
    assert_eq!(
        reader_clone.read(&mut rt.sc()).delta().collect::<Vec<_>>(),
        [SlabMapChange::Insert { key, new_value: &1 }]
    );
}
