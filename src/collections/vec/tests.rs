use std::{cell::Cell, rc::Rc};

use super::*;
use crate::{State, core::Runtime, effect};
use pretty_assertions::assert_eq;

#[test]
fn state_vec_reader_changes() {
    let mut rt = Runtime::new();
    let vec = StateVec::new();
    let mut reader = vec.reader();

    {
        let items = reader.read(&mut rt.sc());
        assert!(items.is_empty());
    }

    {
        let mut items = vec.borrow_mut(rt.ac());
        items.push(1);
        items.push(2);
    }

    {
        let items = reader.read(&mut rt.sc());
        let actual: Vec<_> = items.changes().collect();
        let expected = vec![
            VecChange::Insert {
                index: 0,
                new_value: &1,
            },
            VecChange::Insert {
                index: 1,
                new_value: &2,
            },
        ];
        assert_eq!(actual, expected);
    }

    {
        let mut items = vec.borrow_mut(rt.ac());
        items.set(0, 10);
        items.swap(0, 1);
        items.move_item(0, 1);
        items.remove(1);
        items.insert(0, 20);
    }

    {
        let items = reader.read(&mut rt.sc());
        let actual: Vec<_> = items.changes().collect();
        let expected = vec![
            VecChange::Set {
                index: 0,
                old_value: &1,
                new_value: &10,
            },
            VecChange::Swap { index: (0, 1) },
            VecChange::Move {
                old_index: 0,
                new_index: 1,
            },
            VecChange::Remove {
                index: 1,
                old_value: &2,
            },
            VecChange::Insert {
                index: 0,
                new_value: &20,
            },
        ];
        assert_eq!(actual, expected);
    }
}

#[test]
fn state_vec_reader_peek_does_not_advance() {
    let mut rt = Runtime::new();
    let vec: StateVec<_> = [1, 2].into_iter().collect();
    let mut reader = vec.reader();

    for _ in 0..2 {
        let items = reader.peek(&mut rt.sc());
        assert_eq!(
            items.changes().collect::<Vec<_>>(),
            vec![
                VecChange::Insert {
                    index: 0,
                    new_value: &1,
                },
                VecChange::Insert {
                    index: 1,
                    new_value: &2,
                },
            ]
        );
    }

    {
        let items = reader.read(&mut rt.sc());
        assert_eq!(
            items.changes().collect::<Vec<_>>(),
            vec![
                VecChange::Insert {
                    index: 0,
                    new_value: &1,
                },
                VecChange::Insert {
                    index: 1,
                    new_value: &2,
                },
            ]
        );
    }

    vec.borrow_mut(rt.ac()).push(3);
    {
        let items = reader.peek(&mut rt.sc());
        assert_eq!(
            items.changes().collect::<Vec<_>>(),
            vec![VecChange::Insert {
                index: 2,
                new_value: &3,
            }]
        );
    }

    vec.borrow_mut(rt.ac()).push(4);
    for _ in 0..2 {
        let items = reader.peek(&mut rt.sc());
        assert_eq!(
            items.changes().collect::<Vec<_>>(),
            vec![
                VecChange::Insert {
                    index: 2,
                    new_value: &3,
                },
                VecChange::Insert {
                    index: 3,
                    new_value: &4,
                },
            ]
        );
    }

    {
        let items = reader.read(&mut rt.sc());
        assert_eq!(
            items.changes().collect::<Vec<_>>(),
            vec![
                VecChange::Insert {
                    index: 2,
                    new_value: &3,
                },
                VecChange::Insert {
                    index: 3,
                    new_value: &4,
                },
            ]
        );
    }
    assert_eq!(reader.peek(&mut rt.sc()).changes().collect::<Vec<_>>(), []);
}

#[test]
fn state_vec_reader_clones_have_independent_cursors() {
    let mut rt = Runtime::new();
    let vec = StateVec::new();
    let mut reader = vec.reader();
    let mut unread_clone = reader.clone();

    assert_eq!(reader.read(&mut rt.sc()).changes().collect::<Vec<_>>(), []);
    vec.borrow_mut(rt.ac()).push(1);
    {
        let items = unread_clone.read(&mut rt.sc());
        assert_eq!(
            items.changes().collect::<Vec<_>>(),
            vec![VecChange::Insert {
                index: 0,
                new_value: &1,
            }]
        );
    }
    drop(unread_clone);

    let mut reader_clone = reader.clone();
    {
        let items = reader.read(&mut rt.sc());
        assert_eq!(
            items.changes().collect::<Vec<_>>(),
            vec![VecChange::Insert {
                index: 0,
                new_value: &1,
            }]
        );
    }

    vec.borrow_mut(rt.ac()).push(2);
    {
        let items = reader_clone.read(&mut rt.sc());
        assert_eq!(
            items.changes().collect::<Vec<_>>(),
            vec![
                VecChange::Insert {
                    index: 0,
                    new_value: &1,
                },
                VecChange::Insert {
                    index: 1,
                    new_value: &2,
                },
            ]
        );
    }
    {
        let items = reader.read(&mut rt.sc());
        assert_eq!(
            items.changes().collect::<Vec<_>>(),
            vec![VecChange::Insert {
                index: 1,
                new_value: &2,
            }]
        );
    }

    drop(reader_clone);
    drop(reader);
    vec.borrow_mut(rt.ac()).push(3);
}

#[test]
fn signal_vec_from_scan_peek_retains_changes() {
    let mut rt = Runtime::new();
    let state = State::new(1);
    let state_for_scan = state.clone();
    let vec = SignalVec::from_scan(move |items, sc| {
        let value = state_for_scan.get(sc);
        if items.is_empty() {
            items.push(value);
        } else if items[0] != value {
            items.set(0, value);
        }
    });
    let mut reader = vec.reader();

    let _ = reader.read(&mut rt.sc());
    state.set(2, rt.ac());

    for _ in 0..2 {
        let items = reader.peek(&mut rt.sc());
        assert_eq!(
            items.changes().collect::<Vec<_>>(),
            vec![VecChange::Set {
                index: 0,
                old_value: &1,
                new_value: &2,
            }]
        );
    }

    state.set(3, rt.ac());
    {
        let items = reader.peek(&mut rt.sc());
        assert_eq!(
            items.changes().collect::<Vec<_>>(),
            vec![
                VecChange::Set {
                    index: 0,
                    old_value: &1,
                    new_value: &2,
                },
                VecChange::Set {
                    index: 0,
                    old_value: &2,
                    new_value: &3,
                },
            ]
        );
        assert_eq!(items, [3]);
    }

    {
        let items = reader.read(&mut rt.sc());
        assert_eq!(
            items.changes().collect::<Vec<_>>(),
            vec![
                VecChange::Set {
                    index: 0,
                    old_value: &1,
                    new_value: &2,
                },
                VecChange::Set {
                    index: 0,
                    old_value: &2,
                    new_value: &3,
                },
            ]
        );
    }
}

#[test]
fn signal_vec_from_scan_reader_clones_have_independent_cursors() {
    let mut rt = Runtime::new();
    let state = State::new(1);
    let state_for_scan = state.clone();
    let vec = SignalVec::from_scan(move |items, sc| {
        let value = state_for_scan.get(sc);
        if items.is_empty() {
            items.push(value);
        } else if items[0] != value {
            items.set(0, value);
        }
    });
    let mut reader = vec.reader();

    let _ = reader.read(&mut rt.sc());
    let mut reader_clone = reader.clone();

    state.set(2, rt.ac());
    {
        let items = reader.read(&mut rt.sc());
        assert_eq!(
            items.changes().collect::<Vec<_>>(),
            vec![VecChange::Set {
                index: 0,
                old_value: &1,
                new_value: &2,
            }]
        );
    }

    state.set(3, rt.ac());
    {
        let items = reader_clone.read(&mut rt.sc());
        assert_eq!(
            items.changes().collect::<Vec<_>>(),
            vec![
                VecChange::Set {
                    index: 0,
                    old_value: &1,
                    new_value: &2,
                },
                VecChange::Set {
                    index: 0,
                    old_value: &2,
                    new_value: &3,
                },
            ]
        );
    }
    {
        let items = reader.read(&mut rt.sc());
        assert_eq!(
            items.changes().collect::<Vec<_>>(),
            vec![VecChange::Set {
                index: 0,
                old_value: &2,
                new_value: &3,
            }]
        );
    }

    drop(reader_clone);
    drop(reader);
    state.set(4, rt.ac());
    assert_eq!(vec.borrow(&mut rt.sc()), [4]);
}

#[test]
fn immutable_signal_vec_reader_peek_does_not_advance() {
    let mut rt = Runtime::new();
    let sources: [SignalVec<i32>; 2] = [SignalVec::from(&[1, 2]), vec![1, 2].into()];

    for vec in sources {
        let mut reader = vec.reader();
        {
            let items = reader.peek(&mut rt.sc());
            assert_eq!(
                items.changes().collect::<Vec<_>>(),
                vec![
                    VecChange::Insert {
                        index: 0,
                        new_value: &1,
                    },
                    VecChange::Insert {
                        index: 1,
                        new_value: &2,
                    },
                ]
            );
        }
        {
            let items = reader.read(&mut rt.sc());
            assert_eq!(
                items.changes().collect::<Vec<_>>(),
                vec![
                    VecChange::Insert {
                        index: 0,
                        new_value: &1,
                    },
                    VecChange::Insert {
                        index: 1,
                        new_value: &2,
                    },
                ]
            );
        }
        assert_eq!(reader.peek(&mut rt.sc()).changes().collect::<Vec<_>>(), []);
    }
}

#[test]
fn immutable_signal_vec_reader_clones_have_independent_cursors() {
    let mut rt = Runtime::new();
    let sources: [SignalVec<i32>; 2] = [SignalVec::from(&[1, 2]), vec![1, 2].into()];

    for vec in sources {
        let mut reader = vec.reader();
        let mut reader_clone = reader.clone();

        let _ = reader.read(&mut rt.sc());
        assert_eq!(reader.peek(&mut rt.sc()).changes().collect::<Vec<_>>(), []);
        assert_eq!(
            reader
                .clone()
                .peek(&mut rt.sc())
                .changes()
                .collect::<Vec<_>>(),
            []
        );
        {
            let items = reader_clone.read(&mut rt.sc());
            assert_eq!(
                items.changes().collect::<Vec<_>>(),
                vec![
                    VecChange::Insert {
                        index: 0,
                        new_value: &1,
                    },
                    VecChange::Insert {
                        index: 1,
                        new_value: &2,
                    },
                ]
            );
        }
        assert_eq!(
            reader_clone
                .peek(&mut rt.sc())
                .changes()
                .collect::<Vec<_>>(),
            []
        );
    }
}

#[test]
fn sort_and_drain_changes() {
    let mut rt = Runtime::new();
    let vec = StateVec::new();
    let mut reader = vec.reader();

    {
        let mut items = vec.borrow_mut(rt.ac());
        items.push(3);
        items.push(1);
        items.push(2);
    }
    let _ = reader.read(&mut rt.sc());

    {
        let mut items = vec.borrow_mut(rt.ac());
        items.sort_by_key(|v| *v);
    }
    {
        let items = reader.read(&mut rt.sc());
        let actual: Vec<_> = items.changes().collect();
        let expected = vec![VecChange::Sort(IndexNewToOld::new(&[1, 2, 0]))];
        assert_eq!(actual, expected);
    }

    {
        let mut items = vec.borrow_mut(rt.ac());
        items.sort();
    }
    {
        let items = reader.read(&mut rt.sc());
        let actual: Vec<_> = items.changes().collect();
        let expected: Vec<VecChange<'_, i32>> = Vec::new();
        assert_eq!(actual, expected);
    }

    {
        let mut items = vec.borrow_mut(rt.ac());
        items.drain(1..);
    }
    {
        let items = reader.read(&mut rt.sc());
        let actual: Vec<_> = items.changes().collect();
        let expected = vec![
            VecChange::Remove {
                index: 2,
                old_value: &3,
            },
            VecChange::Remove {
                index: 1,
                old_value: &2,
            },
        ];
        assert_eq!(actual, expected);
    }
}

#[test]
fn signal_vec_from_slice_and_vec_changes() {
    let mut rt = Runtime::new();
    {
        let vec = SignalVec::from(&[1, 2]);
        let mut reader = vec.reader();
        {
            let items = reader.read(&mut rt.sc());
            let actual: Vec<_> = items.changes().collect();
            let expected = vec![
                VecChange::Insert {
                    index: 0,
                    new_value: &1,
                },
                VecChange::Insert {
                    index: 1,
                    new_value: &2,
                },
            ];
            assert_eq!(actual, expected);
        }
        {
            let items = reader.read(&mut rt.sc());
            let actual: Vec<_> = items.changes().collect();
            assert_eq!(actual, vec![]);
        }
    }

    {
        let vec: SignalVec<i32> = vec![1, 2].into();
        let mut reader = vec.reader();
        {
            let items = reader.read(&mut rt.sc());
            let actual: Vec<_> = items.changes().collect();
            let expected = vec![
                VecChange::Insert {
                    index: 0,
                    new_value: &1,
                },
                VecChange::Insert {
                    index: 1,
                    new_value: &2,
                },
            ];
            assert_eq!(actual, expected);
        }

        {
            let items = reader.read(&mut rt.sc());
            let actual: Vec<_> = items.changes().collect();
            assert!(actual.is_empty());
        }
    }
}

#[test]
fn items_basic_apis() {
    let mut rt = Runtime::new();
    let vec = SignalVec::from(&[1, 2, 3]);

    let items = vec.borrow(&mut rt.sc());

    assert_eq!(items.len(), 3);
    assert!(!items.is_empty());
    assert_eq!(items.get(0), Some(&1));
    assert_eq!(items.get(2), Some(&3));
    assert_eq!(items.get(3), None);
    assert_eq!(items[1], 2);

    let iter_values: Vec<i32> = items.iter().copied().collect();
    assert_eq!(iter_values, vec![1, 2, 3]);

    let into_iter_values: Vec<i32> = (&items).into_iter().copied().collect();
    assert_eq!(into_iter_values, vec![1, 2, 3]);

    let debug = format!("{:?}", items);
    assert_eq!(debug, "[1, 2, 3]");
}

#[test]
fn items_partial_eq() {
    let mut rt = Runtime::new();
    let vec = SignalVec::from(&[1, 2, 3]);

    let items = vec.borrow(&mut rt.sc());
    assert!(items == [1, 2, 3]);
    assert!(items == *[1, 2, 3].as_slice());
    assert!(items == vec![1, 2, 3]);
    assert!(items != [1, 2]);
    assert!(items != vec![1, 2]);
}

#[test]
fn items_mut_partial_eq() {
    let mut rt = Runtime::new();
    let vec = StateVec::new();

    {
        let mut items = vec.borrow_mut(rt.ac());
        items.push(1);
        items.push(2);
        items.push(3);
    }

    let items = vec.borrow_mut(rt.ac());
    assert!(items == [1, 2, 3]);
    assert!(items == *[1, 2, 3].as_slice());
    assert!(items == vec![1, 2, 3]);
    assert!(items != [1, 2]);
    assert!(items != vec![1, 2]);
}

#[test]
fn state_vec_releases_old_values_after_the_last_reader_advances() {
    let mut rt = Runtime::new();
    let old = Rc::new(String::from("old"));
    let new = Rc::new(String::from("new"));
    let vec: StateVec<_> = [old.clone()].into_iter().collect();
    let mut reader = vec.reader();
    drop(reader.read(&mut rt.sc()));

    vec.borrow_mut(rt.ac()).set(0, new.clone());
    assert_eq!(Rc::strong_count(&old), 2);

    let items = reader.read(&mut rt.sc());
    assert_eq!(
        items.changes().collect::<Vec<_>>(),
        vec![VecChange::Set {
            index: 0,
            old_value: &old,
            new_value: &new,
        }]
    );
    assert_eq!(Rc::strong_count(&old), 2);
    drop(items);

    assert_eq!(Rc::strong_count(&old), 1);
}

#[test]
fn state_vec_releases_unread_history_without_readers() {
    let mut rt = Runtime::new();
    let old = Rc::new(String::from("old"));
    let vec: StateVec<_> = [old.clone()].into_iter().collect();

    vec.borrow_mut(rt.ac()).set(0, Rc::new(String::from("new")));

    assert_eq!(Rc::strong_count(&old), 1);
}

#[test]
fn state_vec_notifies_only_when_history_is_recorded() {
    let mut rt = Runtime::new();
    let vec = StateVec::<i32>::new();
    let calls = Rc::new(Cell::new(0));
    let _subscription = effect({
        let calls = calls.clone();
        let vec = vec.clone();
        move |sc| {
            drop(vec.borrow(sc));
            calls.set(calls.get() + 1);
        }
    });
    rt.flush();
    assert_eq!(calls.get(), 1);

    drop(vec.borrow_mut(rt.ac()));
    rt.flush();
    assert_eq!(calls.get(), 1);

    vec.borrow_mut(rt.ac()).push(1);
    rt.flush();
    assert_eq!(calls.get(), 2);

    vec.borrow_mut_loose(rt.ac()).push(2);
    rt.flush();
    assert_eq!(calls.get(), 3);
}

#[test]
fn signal_vec_scan_does_not_propagate_unchanged_updates() {
    let mut rt = Runtime::new();
    let source = State::new(1);
    let vec = SignalVec::from_scan({
        let source = source.clone();
        move |items, sc| {
            let value = source.get(sc);
            if items.is_empty() {
                items.push(value);
            } else if items[0] != value {
                items.set(0, value);
            }
        }
    });
    let calls = Rc::new(Cell::new(0));
    let _subscription = effect({
        let calls = calls.clone();
        move |sc| {
            drop(vec.borrow(sc));
            calls.set(calls.get() + 1);
        }
    });
    rt.flush();
    assert_eq!(calls.get(), 1);

    source.set(1, rt.ac());
    rt.flush();
    assert_eq!(calls.get(), 1);

    source.set(2, rt.ac());
    rt.flush();
    assert_eq!(calls.get(), 2);
}

#[test]
fn state_vec_serde_preserves_values_and_starts_readers_as_initial() {
    let vec: StateVec<_> = [1, 2].into_iter().collect();
    let json = serde_json::to_string(&vec).unwrap();
    assert_eq!(json, "[1,2]");

    let restored: StateVec<i32> = serde_json::from_str(&json).unwrap();
    let mut reader = restored.reader();
    let mut rt = Runtime::new();
    assert_eq!(
        reader.read(&mut rt.sc()).changes().collect::<Vec<_>>(),
        vec![
            VecChange::Insert {
                index: 0,
                new_value: &1,
            },
            VecChange::Insert {
                index: 1,
                new_value: &2,
            },
        ]
    );
}

#[test]
fn state_vec_serialization_reports_borrow_conflict() {
    let mut rt = Runtime::new();
    let vec: StateVec<_> = [1, 2].into_iter().collect();
    let _items = vec.borrow_mut(rt.ac());

    assert!(serde_json::to_string(&vec).is_err());
}
