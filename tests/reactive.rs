use reactive_fn::*;
use std::{cell::RefCell, rc::Rc};

#[test]
fn test_for_each() {
    let cell = ReCell::new(0);
    let re = cell.to_re();
    let r = record(&re);

    cell.set(5);
    cell.set(10);

    assert_eq!(r.finish(), vec![0, 5, 10]);
}

#[test]
fn test_map() {
    let cell = ReCell::new(0);
    let re = cell.to_re().map(|x| x + 1);
    let r = record(&re);

    cell.set(5);
    cell.set(10);

    assert_eq!(r.finish(), vec![1, 6, 11]);
}
#[test]
fn test_cell2() {
    let cell1 = ReCell::new(1);
    let cell2 = ReCell::new(2);

    let re = {
        let cell1 = cell1.clone();
        let cell2 = cell2.clone();
        Re::new(move |ctx| cell1.get(ctx) + cell2.get(ctx))
    };
    let r = record(&re);

    cell1.set(5);
    cell2.set(10);

    assert_eq!(r.finish(), vec![1 + 2, 5 + 2, 5 + 10]);
}

#[test]
fn test_same_value() {
    let cell = ReCell::new(5);
    let re = cell.to_re();
    let r = record(&re);

    cell.set(5);
    cell.set(5);

    assert_eq!(r.finish(), vec![5, 5, 5]);
}
#[test]
fn test_dedup() {
    let cell = ReCell::new(5);
    let re = cell.to_re().dedup().cloned();
    let r = record(&re);

    cell.set(5);
    cell.set(5);
    cell.set(6);
    cell.set(6);
    cell.set(5);

    assert_eq!(r.finish(), vec![5, 6, 5]);
}

struct Recorder<T> {
    rc: Rc<RefCell<Vec<T>>>,
    unbind: Unbind,
}
fn record<T>(s: &Re<T>) -> Recorder<T> {
    let rc = Rc::new(RefCell::new(Vec::new()));
    let r = rc.clone();
    let unbind = s.for_each(move |x| {
        r.borrow_mut().push(x);
    });
    Recorder { rc, unbind }
}
impl<T> Recorder<T> {
    fn finish(self) -> Vec<T> {
        let Recorder { rc, unbind } = self;
        drop(unbind);
        if let Ok(cell) = Rc::try_unwrap(rc) {
            cell.into_inner()
        } else {
            panic!("for_each not complated.");
        }
    }
}
