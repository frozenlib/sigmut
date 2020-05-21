mod test_utils;

use self::test_utils::*;
use reactive_fn::*;

#[test]
fn re_constant() {
    let a = Re::constant(2);
    let r = record(&a);
    assert_eq!(r.finish(), vec![2]);
}

#[test]
fn re_new() {
    let a = ReCell::new(2);
    let a_ = a.clone();
    let b = Re::new(move |ctx| a_.get(ctx));
    let r = record(&b);

    a.set_and_update(5);
    a.set_and_update(7);

    assert_eq!(r.finish(), vec![2, 5, 7]);
}

#[test]
fn re_new_cell2() {
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
fn re_map() {
    let a = ReCell::new(2);
    let b = a.to_re().map(|x| x * 2);
    let r = record(&b);

    a.set_and_update(5);
    a.set_and_update(7);

    assert_eq!(r.finish(), vec![4, 10, 14]);
}

#[test]
fn re_flat_map() {
    let a = [ReCell::new(5), ReCell::new(10)];
    let a_ = a.clone();

    let b = ReCell::new(0);

    let c = b.to_re().flat_map(move |x| a_[x].to_re());
    let r = record(&c);

    a[0].set_and_update(6);
    a[1].set_and_update(12);

    a[0].set_and_update(7);
    a[1].set_and_update(13);

    b.set_and_update(1);

    a[0].set_and_update(8);
    a[1].set_and_update(14);

    assert_eq!(r.finish(), vec![5, 6, 7, 13, 14]);
}

#[test]
fn re_cahced() {
    let cell = ReCell::new(0);
    let re = cell.to_re().map(|x| x + 1).cached().cloned();
    let r = record(&re);

    cell.set_and_update(5);
    cell.set_and_update(10);

    assert_eq!(r.finish(), vec![1, 6, 11]);
}

#[test]
fn re_scan() {
    let cell = ReCell::new(2);
    let re = cell.to_re().scan(10, |s, x| s + x).cloned();
    let r = record(&re);

    cell.set_and_update(3);
    cell.set_and_update(4);
    cell.set_and_update(5);

    assert_eq!(r.finish(), vec![12, 15, 19, 24]);
}
#[test]
fn re_filter_scan() {
    let cell = ReCell::new(2);
    let re = cell
        .to_re()
        .filter_scan(10, |_s, x| x % 2 != 0, |s, x| s + x)
        .cloned();
    let r = record(&re);

    cell.set_and_update(3);
    cell.set_and_update(4);
    cell.set_and_update(5);
    cell.set_and_update(6);

    assert_eq!(r.finish(), vec![10, 13, 18]);
}

#[test]
fn re_same_value() {
    let cell = ReCell::new(5);
    let re = cell.to_re();
    let r = record(&re);

    cell.set_and_update(5);
    cell.set_and_update(5);

    assert_eq!(r.finish(), vec![5, 5, 5]);
}
#[test]
fn re_dedup() {
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

#[test]
fn re_dedup_by_key_1() {
    let cell = ReCell::new((5, 1));
    let re = cell.to_re().dedup_by_key(|&(x, _)| x).cloned();
    let r = record(&re);

    cell.set_and_update((5, 2));
    cell.set_and_update((6, 2));
    cell.set_and_update((6, 2));
    cell.set_and_update((6, 1));
    cell.set_and_update((5, 2));

    assert_eq!(r.finish(), vec![(5, 1), (6, 2), (5, 2)]);
}

#[test]
fn re_dedup_by_key_2() {
    let cell = ReCell::new((5, 1));
    let re = cell.to_re().dedup_by_key(|&(x, _)| x).cloned();

    cell.set_and_update((5, 2));
    let r = record(&re); // current value is (5, 2), not (5, 1).
    cell.set_and_update((6, 2));
    cell.set_and_update((6, 2));
    cell.set_and_update((6, 1));
    cell.set_and_update((5, 2));

    assert_eq!(r.finish(), vec![(5, 2), (6, 2), (5, 2)]);
}

#[test]
fn re_dedup_by() {
    let cell = ReCell::new((5, 1));
    let re = cell
        .to_re()
        .dedup_by(|&(x1, _), &(x2, _)| x1 == x2)
        .cloned();
    let r = record(&re);

    cell.set_and_update((5, 2));
    cell.set_and_update((6, 2));
    cell.set_and_update((6, 2));
    cell.set_and_update((6, 1));
    cell.set_and_update((5, 2));

    assert_eq!(r.finish(), vec![(5, 1), (6, 2), (5, 2)]);
}

#[test]
fn re_fold() {
    let cell = ReCell::new(1);
    let fold = cell.to_re().fold(2, |s, x| s + x);

    cell.set_and_update(5);
    cell.set_and_update(10);

    assert_eq!(fold.stop(), 18);
}

#[test]
fn re_for_each() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let cell = ReCell::new(0);
    let vs = Rc::new(RefCell::new(Vec::new()));

    let vs_send = vs.clone();

    let r = cell.to_re().for_each(move |x| {
        vs_send.borrow_mut().push(x);
    });

    cell.set_and_update(5);
    cell.set_and_update(10);

    drop(r);
    assert_eq!(*vs.borrow(), vec![0, 5, 10]);

    cell.set_and_update(15);
    assert_eq!(*vs.borrow(), vec![0, 5, 10]);
}

#[test]
fn re_for_each_by() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let cell = ReCell::new(0);
    let vs = Rc::new(RefCell::new(Vec::new()));

    let vs_send1 = vs.clone();
    let vs_send2 = vs.clone();

    let r = cell.to_re().for_each_by(
        move |x| {
            vs_send1.borrow_mut().push(x);
            x + 1
        },
        move |x| {
            vs_send2.borrow_mut().push(x);
        },
    );

    cell.set_and_update(5);
    cell.set_and_update(10);

    drop(r);
    assert_eq!(*vs.borrow(), vec![0, 1, 5, 6, 10, 11]);

    cell.set_and_update(15);
    assert_eq!(*vs.borrow(), vec![0, 1, 5, 6, 10, 11]);
}
