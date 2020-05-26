use reactive_fn::*;

#[test]
fn re_borrow_constant() {
    let r = ReBorrow::constant(2).to_vec();
    assert_eq!(r.stop(), vec![2]);
}
#[test]
fn re_borrow_new() {
    let a = ReRefCell::new(2);
    let r = ReBorrow::new(a.clone(), move |a, ctx| a.borrow(ctx)).to_vec();

    a.set_and_update(5);
    a.set_and_update(7);

    assert_eq!(r.stop(), vec![2, 5, 7]);
}

#[test]
fn re_borrow_map() {
    let a = ReRefCell::new(2);
    let r = a.to_re_borrow().map(|x| x * 2).to_vec();

    a.set_and_update(5);
    a.set_and_update(7);

    assert_eq!(r.stop(), vec![4, 10, 14]);
}

#[test]
fn re_borrow_map_ref() {
    let a = ReRefCell::new((2, 3));
    let r = a.to_re_borrow().map_ref(|x| &x.0).to_vec();

    a.set_and_update((5, 8));
    a.set_and_update((7, 1));

    assert_eq!(r.stop(), vec![2, 5, 7]);
}

#[test]
fn re_borrow_flat_map() {
    let a = [ReCell::new(5), ReCell::new(10)];
    let a_ = a.clone();

    let b = ReRefCell::new(0);

    let r = b.to_re_borrow().flat_map(move |&x| a_[x].to_re()).to_vec();

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
fn re_borrow_cloned() {
    let cell = ReRefCell::new(2);
    let r = cell.to_re_borrow().cloned().to_vec();

    cell.set_and_update(3);
    cell.set_and_update(4);
    cell.set_and_update(5);

    assert_eq!(r.stop(), vec![2, 3, 4, 5]);
}

#[test]
fn re_borrow_scan() {
    let cell = ReRefCell::new(2);
    let r = cell.to_re_borrow().scan(10, |s, x| s + x).to_vec();

    cell.set_and_update(3);
    cell.set_and_update(4);
    cell.set_and_update(5);

    assert_eq!(r.stop(), vec![12, 15, 19, 24]);
}
#[test]
fn re_borrow_filter_scan() {
    let cell = ReRefCell::new(2);
    let r = cell
        .to_re_borrow()
        .filter_scan(10, |_s, x| x % 2 != 0, |s, x| s + x)
        .to_vec();

    cell.set_and_update(3);
    cell.set_and_update(4);
    cell.set_and_update(5);
    cell.set_and_update(6);

    assert_eq!(r.stop(), vec![10, 13, 18]);
}
#[test]
fn re_borrow_fold() {
    let cell = ReRefCell::new(1);
    let fold = cell.to_re_borrow().fold(2, |s, x| s + x);

    cell.set_and_update(5);
    cell.set_and_update(10);

    assert_eq!(fold.stop(), 18);
}

#[test]
fn re_borrow_to_vec() {
    let cell = ReRefCell::new(1);
    let fold = cell.to_re_ref().to_vec();

    cell.set_and_update(2);
    cell.set_and_update(1);
    cell.set_and_update(3);

    assert_eq!(fold.stop(), vec![1, 2, 1, 3]);
}

#[test]
fn re_borrow_for_each() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let cell = ReRefCell::new(0);
    let vs = Rc::new(RefCell::new(Vec::new()));

    let vs_send = vs.clone();

    let r = cell.to_re_borrow().for_each(move |&x| {
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
fn re_borrow_hot() {
    let cell = ReRefCell::new(1);
    let re = cell.to_re_borrow().scan(0, |s, x| s + x);

    let hot = re.hot();

    cell.set_and_update(2);
    cell.set_and_update(10);

    assert_eq!(hot.to_vec().stop(), vec![13]);
}

#[test]
fn re_borrow_hot_no() {
    let cell = ReRefCell::new(1);
    let re = cell.to_re_borrow().scan(0, |s, x| s + x);

    cell.set_and_update(2);
    cell.set_and_update(10);

    assert_eq!(re.to_vec().stop(), vec![10]);
}

#[test]
fn re_ref_flatten() {
    let cell = ReRefCell::new(Re::constant(1));

    let vs = cell.to_re_borrow().flatten().to_vec();

    cell.set_and_update(Re::constant(2));
    cell.set_and_update(Re::constant(3));
    cell.set_and_update(Re::constant(4));
    cell.set_and_update(Re::constant(5));

    assert_eq!(vs.stop(), vec![1, 2, 3, 4, 5]);
}
