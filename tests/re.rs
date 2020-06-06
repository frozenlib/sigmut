use reactive_fn::*;
use std::collections::HashSet;

#[test]
fn re_constant() {
    let r = Re::constant(2).to_vec();
    assert_eq!(r.stop(), vec![2]);
}

#[test]
fn re_new() {
    let a = ReCell::new(2);
    let a_ = a.clone();
    let r = Re::new(move |ctx| a_.get(ctx)).to_vec();

    a.set_and_update(5);
    a.set_and_update(7);

    assert_eq!(r.stop(), vec![2, 5, 7]);
}

#[test]
fn re_new_cell2() {
    let cell1 = ReCell::new(1);
    let cell2 = ReCell::new(2);

    let r = {
        let cell1 = cell1.clone();
        let cell2 = cell2.clone();
        Re::new(move |ctx| cell1.get(ctx) + cell2.get(ctx)).to_vec()
    };

    cell1.set_and_update(5);
    cell2.set_and_update(10);

    assert_eq!(r.stop(), vec![1 + 2, 5 + 2, 5 + 10]);
}

#[test]
fn re_map() {
    let a = ReCell::new(2);
    let r = a.to_re().map(|x| x * 2).to_vec();

    a.set_and_update(5);
    a.set_and_update(7);

    assert_eq!(r.stop(), vec![4, 10, 14]);
}

#[test]
fn re_flat_map() {
    let a = [ReCell::new(5), ReCell::new(10)];
    let a_ = a.clone();

    let b = ReCell::new(0);

    let r = b.to_re().flat_map(move |x| a_[x].to_re()).to_vec();

    a[0].set_and_update(6);
    a[1].set_and_update(12);

    a[0].set_and_update(7);
    a[1].set_and_update(13);

    b.set_and_update(1);

    a[0].set_and_update(8);
    a[1].set_and_update(14);

    assert_eq!(r.stop(), vec![5, 6, 7, 13, 14]);
}

#[test]
fn re_cahced() {
    let cell = ReCell::new(0);
    let r = cell.to_re().map(|x| x + 1).cached().to_vec();

    cell.set_and_update(5);
    cell.set_and_update(10);

    assert_eq!(r.stop(), vec![1, 6, 11]);
}

#[test]
fn re_scan() {
    let cell = ReCell::new(2);
    let r = cell.to_re().scan(10, |s, x| s + x).to_vec();

    cell.set_and_update(3);
    cell.set_and_update(4);
    cell.set_and_update(5);

    assert_eq!(r.stop(), vec![12, 15, 19, 24]);
}
#[test]
fn re_filter_scan() {
    let cell = ReCell::new(2);
    let r = cell
        .to_re()
        .filter_scan(10, |_s, x| x % 2 != 0, |s, x| s + x)
        .to_vec();

    cell.set_and_update(3);
    cell.set_and_update(4);
    cell.set_and_update(5);
    cell.set_and_update(6);

    assert_eq!(r.stop(), vec![10, 13, 18]);
}

#[test]
fn re_same_value() {
    let cell = ReCell::new(5);
    let r = cell.to_re().to_vec();

    cell.set_and_update(5);
    cell.set_and_update(5);

    assert_eq!(r.stop(), vec![5, 5, 5]);
}
#[test]
fn re_dedup() {
    let cell = ReCell::new(5);
    let r = cell.to_re().dedup().to_vec();

    cell.set_and_update(5);
    cell.set_and_update(5);
    cell.set_and_update(6);
    cell.set_and_update(6);
    cell.set_and_update(5);

    assert_eq!(r.stop(), vec![5, 6, 5]);
}

#[test]
fn re_dedup_by_key_1() {
    let cell = ReCell::new((5, 1));
    let r = cell.to_re().dedup_by_key(|&(x, _)| x).to_vec();

    cell.set_and_update((5, 2));
    cell.set_and_update((6, 2));
    cell.set_and_update((6, 2));
    cell.set_and_update((6, 1));
    cell.set_and_update((5, 2));

    assert_eq!(r.stop(), vec![(5, 1), (6, 2), (5, 2)]);
}

#[test]
fn re_dedup_by_key_2() {
    let cell = ReCell::new((5, 1));
    let re = cell.to_re().dedup_by_key(|&(x, _)| x);

    cell.set_and_update((5, 2));
    let r = re.to_vec(); // current value is (5, 2), not (5, 1).
    cell.set_and_update((6, 2));
    cell.set_and_update((6, 2));
    cell.set_and_update((6, 1));
    cell.set_and_update((5, 2));

    assert_eq!(r.stop(), vec![(5, 2), (6, 2), (5, 2)]);
}

#[test]
fn re_dedup_by() {
    let cell = ReCell::new((5, 1));
    let r = cell
        .to_re()
        .dedup_by(|&(x1, _), &(x2, _)| x1 == x2)
        .to_vec();

    cell.set_and_update((5, 2));
    cell.set_and_update((6, 2));
    cell.set_and_update((6, 2));
    cell.set_and_update((6, 1));
    cell.set_and_update((5, 2));

    assert_eq!(r.stop(), vec![(5, 1), (6, 2), (5, 2)]);
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
fn re_collect_to() {
    let cell = ReCell::new(1);
    let fold = cell.to_re().collect_to(HashSet::new());

    cell.set_and_update(2);
    cell.set_and_update(1);
    cell.set_and_update(3);

    let e: HashSet<_> = vec![1, 2, 3].into_iter().collect();
    assert_eq!(fold.stop(), e);
}
#[test]
fn re_collect() {
    let cell = ReCell::new(1);
    let fold = cell.to_re().collect_to(HashSet::new());

    cell.set_and_update(2);
    cell.set_and_update(1);
    cell.set_and_update(3);

    let e: HashSet<_> = vec![1, 2, 3].into_iter().collect();
    let a: HashSet<_> = fold.stop();
    assert_eq!(a, e);
}

#[test]
fn re_to_vec() {
    let cell = ReCell::new(1);
    let fold = cell.to_re().to_vec();

    cell.set_and_update(2);
    cell.set_and_update(1);
    cell.set_and_update(3);

    assert_eq!(fold.stop(), vec![1, 2, 1, 3]);
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
fn re_head_tail() {
    let a = ReCell::new(2);
    let (head, tail) = a.to_re().head_tail();
    let r = tail.to_vec();

    a.set_and_update(5);
    a.set_and_update(7);

    assert_eq!(head, 2);
    assert_eq!(r.stop(), vec![5, 7]);
}

#[test]
fn re_hot() {
    let cell = ReCell::new(1);
    let re = cell.to_re().scan(0, |s, x| s + x);

    let hot = re.hot();

    cell.set_and_update(2);
    cell.set_and_update(10);

    assert_eq!(hot.to_vec().stop(), vec![13]);
}

#[test]
fn re_hot_no() {
    let cell = ReCell::new(1);
    let re = cell.to_re().scan(0, |s, x| s + x);

    cell.set_and_update(2);
    cell.set_and_update(10);

    assert_eq!(re.to_vec().stop(), vec![10]);
}

#[test]
fn re_flatten() {
    let cell = ReRefCell::new(Re::constant(1));

    let vs = cell.to_re_borrow().cloned().flatten().to_vec();

    cell.set_and_update(Re::constant(2));
    cell.set_and_update(Re::constant(3));
    cell.set_and_update(Re::constant(4));
    cell.set_and_update(Re::constant(5));

    assert_eq!(vs.stop(), vec![1, 2, 3, 4, 5]);
}
