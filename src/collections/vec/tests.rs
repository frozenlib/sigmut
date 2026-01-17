use super::*;
use crate::core::Runtime;

#[test]
fn state_vec_reader_changes() {
    let mut rt = Runtime::new();
    let vec = StateVec::new();
    let mut reader = vec.reader();

    {
        let items0 = reader.read(&mut rt.sc());
        assert!(items0.is_empty());
    }

    {
        let mut items = vec.borrow_mut(rt.ac());
        items.push(1);
        items.push(2);
    }

    {
        let items1 = reader.read(&mut rt.sc());
        let changes: Vec<_> = items1.changes().collect();
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
        assert_eq!(changes, expected);
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
        let items2 = reader.read(&mut rt.sc());
        let changes: Vec<_> = items2.changes().collect();
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
        assert_eq!(changes, expected);
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
        let items1 = reader.read(&mut rt.sc());
        let changes: Vec<_> = items1.changes().collect();
        let expected = vec![VecChange::Sort(IndexNewToOld::new(&[1, 2, 0]))];
        assert_eq!(changes, expected);
    }

    {
        let mut items = vec.borrow_mut(rt.ac());
        items.sort();
    }
    {
        let items2 = reader.read(&mut rt.sc());
        let changes: Vec<_> = items2.changes().collect();
        let expected: Vec<VecChange<'_, i32>> = Vec::new();
        assert_eq!(changes, expected);
    }

    {
        let mut items = vec.borrow_mut(rt.ac());
        items.drain(1..);
    }
    {
        let items3 = reader.read(&mut rt.sc());
        let changes: Vec<_> = items3.changes().collect();
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
        assert_eq!(changes, expected);
    }
}
