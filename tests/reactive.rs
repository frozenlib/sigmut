mod test_utils;

use self::test_utils::*;
use reactive_fn::*;

#[test]
fn re_new() {
    let x = ReCell::new(2);
    let x2 = x.clone();
    let r = record(&Re::new(move |ctx| x2.get(ctx)));

    x.set_and_update(5);
    x.set_and_update(7);

    assert_eq!(r.finish(), vec![2, 5, 7]);
}

// =========================================

#[test]
fn test_for_each() {
    let cell = ReCell::new(0);
    let re = cell.to_re();
    let r = record(&re);

    cell.set_and_update(5);
    cell.set_and_update(10);

    assert_eq!(r.finish(), vec![0, 5, 10]);
}

#[test]
fn test_map() {
    let cell = ReCell::new(0);
    let re = cell.to_re().map(|x| x + 1);
    let r = record(&re);

    cell.set_and_update(5);
    cell.set_and_update(10);

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

    cell1.set_and_update(5);
    cell2.set_and_update(10);

    assert_eq!(r.finish(), vec![1 + 2, 5 + 2, 5 + 10]);
}
#[test]
fn test_cahced() {
    let cell = ReCell::new(0);
    let re = cell.to_re().map(|x| x + 1).cached().cloned();
    let r = record(&re);

    cell.set_and_update(5);
    cell.set_and_update(10);

    assert_eq!(r.finish(), vec![1, 6, 11]);
}

#[test]
fn test_same_value() {
    let cell = ReCell::new(5);
    let re = cell.to_re();
    let r = record(&re);

    cell.set_and_update(5);
    cell.set_and_update(5);

    assert_eq!(r.finish(), vec![5, 5, 5]);
}
#[test]
fn test_dedup() {
    let cell = ReCell::new(5);
    let re = cell.to_re().dedup().cloned();
    let r = record(&re);

    cell.set_and_update(5);
    cell.set_and_update(5);
    cell.set_and_update(6);
    cell.set_and_update(6);
    cell.set_and_update(5);

    assert_eq!(r.finish(), vec![5, 6, 5]);
}
