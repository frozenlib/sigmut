use super::*;
use crate::{State, core::Runtime};
use std::{cell::Cell, rc::Rc};

#[test]
fn state_slab_map_reader_changes() {
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
        let changes: Vec<_> = items1.changes().collect();
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
        let changes: Vec<_> = items2.changes().collect();
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
