use super::ObsVecCell;
use crate::{
    collections::vec::{IndexMapping, ObsVecChange, ObsVecItems},
    core::Runtime,
};

#[test]
fn changes_from_emtpy() {
    let mut dc = Runtime::new();
    let cell = ObsVecCell::new();
    let mut s = cell.session();

    cell.borrow_mut(&mut dc.ac()).push(10);
    cell.borrow_mut(&mut dc.ac()).push(20);
    let a = vec![
        Log::Insert {
            index: 0,
            new_value: 10,
        },
        Log::Insert {
            index: 1,
            new_value: 20,
        },
    ];
    s.read(|r, _| assert_eq_changes(&r, &a), dc.ac().oc());
}

#[test]
fn changes_from_last_read() {
    let mut dc = Runtime::new();
    let cell = ObsVecCell::new();
    let mut s = cell.session();
    cell.borrow_mut(&mut dc.ac()).push(10);
    s.read(|_, _| {}, dc.ac().oc());

    cell.borrow_mut(&mut dc.ac()).push(20);
    let a = vec![Log::Insert {
        index: 1,
        new_value: 20,
    }];
    s.read(|r, _| assert_eq_changes(&r, &a), dc.ac().oc());
}

#[test]
fn changes_from_last_read2() {
    let mut dc = Runtime::new();
    let cell = ObsVecCell::new();
    let mut s0 = cell.session();
    let mut s1 = cell.session();
    cell.borrow_mut(&mut dc.ac()).push(10);
    s0.read(|_, _| {}, dc.ac().oc());
    s1.read(|_, _| {}, dc.ac().oc());

    cell.borrow_mut(&mut dc.ac()).push(20);
    drop(s1);

    let a = vec![Log::Insert {
        index: 1,
        new_value: 20,
    }];
    s0.read(|r, _| assert_eq_changes(&r, &a), dc.ac().oc());
}

#[test]
fn no_chnages() {
    let mut dc = Runtime::new();
    let cell = ObsVecCell::new();
    cell.borrow_mut(&mut dc.ac()).push(10);
    assert_eq!(cell.changes_len(), 0);
}

#[test]
fn push_changes() {
    let mut dc = Runtime::new();
    let cell = ObsVecCell::new();
    let mut s = cell.session();
    s.read(|_, _| {}, dc.ac().oc());

    cell.borrow_mut(&mut dc.ac()).push(10);
    assert_eq!(cell.changes_len(), 1);
}

#[test]
fn clear_changes_on_read_session() {
    let mut dc = Runtime::new();
    let cell = ObsVecCell::new();
    let mut s = cell.session();
    s.read(|_, _| {}, dc.ac().oc());

    cell.borrow_mut(&mut dc.ac()).push(10);
    assert_eq!(cell.changes_len(), 1);

    s.read(|_, _| {}, dc.ac().oc());
    assert_eq!(cell.changes_len(), 0);
}
#[test]
fn clear_changes_on_drop_session() {
    let mut dc = Runtime::new();
    let cell = ObsVecCell::new();
    let mut s = cell.session();
    s.read(|_, _| {}, dc.ac().oc());

    cell.borrow_mut(&mut dc.ac()).push(10);
    assert_eq!(cell.changes_len(), 1);

    drop(s);
    assert_eq!(cell.changes_len(), 0);
}

#[test]
fn new_session_in_read() {
    let mut dc = Runtime::new();
    let cell = ObsVecCell::new();
    let mut s = cell.session();
    s.read(
        |_, _| {
            let _s = cell.session();
        },
        dc.ac().oc(),
    );
    cell.borrow_mut(&mut dc.ac()).push(10);
    assert_eq!(cell.changes_len(), 1);
}

#[test]
fn drop_session_in_read() {
    let mut dc = Runtime::new();
    let cell: ObsVecCell<u32> = ObsVecCell::new();
    let mut s1 = cell.obs().session();
    let mut s2 = cell.obs().session();
    s1.read(|_, _| {}, dc.ac().oc());
    s2.read(|_, _| {}, dc.ac().oc());
    cell.borrow_mut(&mut dc.ac()).push(0);
    s2.read(
        |_, _| {
            drop(s1);
        },
        dc.ac().oc(),
    );
    assert_eq!(cell.changes_len(), 0);
}

#[test]
fn new_session_in_borrow_mut() {
    let mut dc = Runtime::new();
    let cell = ObsVecCell::<u32>::new();
    let mut s = cell.session();
    s.read(|_, _| {}, dc.ac().oc());
    let _b = cell.borrow_mut(&mut dc.ac());
    let _s = cell.session();
    drop(s);
    drop(_b);
    assert_eq!(cell.changes_len(), 0);
}

#[test]
fn drop_session_in_borrow_mut() {
    let mut dc = Runtime::new();
    let cell: ObsVecCell<u32> = ObsVecCell::new();
    let mut s = cell.obs().session();
    s.read(|_, _| {}, dc.ac().oc());
    let mut b = cell.borrow_mut(&mut dc.ac());
    b.push(0);
    drop(s);
    drop(b);
    assert_eq!(cell.changes_len(), 0);
}

#[test]
fn read_in_read() {
    let mut dc = Runtime::new();
    let cell = ObsVecCell::new();
    let mut s0 = cell.session();
    let mut s1 = cell.session();
    s0.read(|_, oc| s1.read(|_, _| {}, oc), dc.ac().oc());
    cell.borrow_mut(&mut dc.ac()).push(10);
    assert_eq!(cell.changes_len(), 1);
}

fn assert_eq_changes<T>(r: &ObsVecItems<T>, a: &[Log<T>])
where
    T: std::fmt::Debug + PartialEq + Clone,
{
    let mut e = Vec::new();
    r.changes(|x| e.push(Log::new(x)));
    assert_eq!(e, a);
}

#[derive(Debug, Clone, PartialEq)]
enum Log<T> {
    Insert {
        index: usize,
        new_value: T,
    },
    Remove {
        index: usize,
        old_value: T,
    },
    Set {
        index: usize,
        new_value: T,
        old_value: T,
    },
    Move {
        old_index: usize,
        new_index: usize,
    },
    Swap {
        index: (usize, usize),
    },
    Sort(IndexMapping),
}

impl<T: Clone> Log<T> {
    fn new(x: ObsVecChange<T>) -> Self {
        match x {
            ObsVecChange::Insert { index, new_value } => Self::Insert {
                index,
                new_value: new_value.clone(),
            },
            ObsVecChange::Remove { index, old_value } => Self::Remove {
                index,
                old_value: old_value.clone(),
            },
            ObsVecChange::Set {
                index,
                new_value,
                old_value,
            } => Self::Set {
                index,
                new_value: new_value.clone(),
                old_value: old_value.clone(),
            },
            ObsVecChange::Move {
                old_index,
                new_index,
            } => Self::Move {
                old_index,
                new_index,
            },
            ObsVecChange::Swap { index } => Self::Swap { index },
            ObsVecChange::Sort(m) => Self::Sort(m.clone()),
        }
    }
}

#[test]
fn serialize() {
    let e: ObsVecCell<u32> = ObsVecCell::from([1, 2, 3]);
    let s = serde_json::to_string(&e).unwrap();
    let a: ObsVecCell<u32> = serde_json::from_str(&s).unwrap();
    assert_eq!(a.debug(), e.debug());
}
