use reactive_fn::*;

#[test]
fn re_borrow_constant() {
    let r = DynObsBorrow::constant(2).collect_vec();
    assert_eq!(r.stop(), vec![2]);
}
#[test]
fn re_borrow_new() {
    let a = ReRefCell::new(2);
    let r = DynObsBorrow::new(a.clone(), move |a, cx| a.borrow(cx)).collect_vec();

    a.set(5);
    a.set(7);

    assert_eq!(r.stop(), vec![2, 5, 7]);
}

#[test]
fn re_borrow_map() {
    let a = ReRefCell::new(2);
    let r = a.re_borrow().map(|x| x * 2).collect_vec();

    a.set(5);
    a.set(7);

    assert_eq!(r.stop(), vec![4, 10, 14]);
}

#[test]
fn re_borrow_map_ref() {
    let a = ReRefCell::new((2, 3));
    let r = a.re_borrow().map_ref(|x| &x.0).collect_vec();

    a.set((5, 8));
    a.set((7, 1));

    assert_eq!(r.stop(), vec![2, 5, 7]);
}

#[test]
fn re_borrow_flat_map() {
    let a = [ReCell::new(5), ReCell::new(10)];
    let a_ = a.clone();

    let b = ReRefCell::new(0);

    let r = b.re_borrow().flat_map(move |&x| a_[x].re()).collect_vec();

    a[0].set(6);
    a[1].set(12);

    a[0].set(7);
    a[1].set(13);

    b.set(1);

    a[0].set(8);
    a[1].set(14);

    assert_eq!(r.stop(), vec![5, 6, 7, 13, 14]);
}

#[test]
fn re_borrow_cloned() {
    let cell = ReRefCell::new(2);
    let r = cell.re_borrow().cloned().collect_vec();

    cell.set(3);
    cell.set(4);
    cell.set(5);

    assert_eq!(r.stop(), vec![2, 3, 4, 5]);
}

#[test]
fn re_borrow_scan() {
    let cell = ReRefCell::new(2);
    let r = cell.re_borrow().scan(10, |s, x| s + x).collect_vec();

    cell.set(3);
    cell.set(4);
    cell.set(5);

    assert_eq!(r.stop(), vec![12, 15, 19, 24]);
}
#[test]
fn re_borrow_filter_scan() {
    let cell = ReRefCell::new(2);
    let r = cell
        .re_borrow()
        .filter_scan(10, |_s, x| x % 2 != 0, |s, x| s + x)
        .collect_vec();

    cell.set(3);
    cell.set(4);
    cell.set(5);
    cell.set(6);

    assert_eq!(r.stop(), vec![10, 13, 18]);
}
#[test]
fn re_borrow_fold() {
    let cell = ReRefCell::new(1);
    let fold = cell.re_borrow().fold(2, |s, x| s + x);

    cell.set(5);
    cell.set(10);

    assert_eq!(fold.stop(), 18);
}

#[test]
fn re_borrow_collect_vec() {
    let cell = ReRefCell::new(1);
    let fold = cell.re_ref().collect_vec();

    cell.set(2);
    cell.set(1);
    cell.set(3);

    assert_eq!(fold.stop(), vec![1, 2, 1, 3]);
}

#[test]
fn re_borrow_for_each() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let cell = ReRefCell::new(0);
    let vs = Rc::new(RefCell::new(Vec::new()));

    let vs_send = vs.clone();

    let r = cell.re_borrow().for_each(move |&x| {
        vs_send.borrow_mut().push(x);
    });

    cell.set(5);
    cell.set(10);

    drop(r);
    assert_eq!(*vs.borrow(), vec![0, 5, 10]);

    cell.set(15);
    assert_eq!(*vs.borrow(), vec![0, 5, 10]);
}

#[test]
fn re_borrow_hot() {
    let cell = ReRefCell::new(1);
    let re = cell.re_borrow().scan(0, |s, x| s + x);

    let hot = re.hot();

    cell.set(2);
    cell.set(10);

    assert_eq!(hot.collect_vec().stop(), vec![13]);
}

#[test]
fn re_borrow_hot_no() {
    let cell = ReRefCell::new(1);
    let re = cell.re_borrow().scan(0, |s, x| s + x);

    cell.set(2);
    cell.set(10);

    assert_eq!(re.collect_vec().stop(), vec![10]);
}

#[test]
fn re_borrow_flatten() {
    let cell = ReRefCell::new(DynObs::constant(1));

    let vs = cell.re_borrow().flatten().collect_vec();

    cell.set(DynObs::constant(2));
    cell.set(DynObs::constant(3));
    cell.set(DynObs::constant(4));
    cell.set(DynObs::constant(5));

    assert_eq!(vs.stop(), vec![1, 2, 3, 4, 5]);
}

#[test]
fn re_borrow_head_tail_with() {
    let a = ReRefCell::new(2);
    let (head, tail) = BindScope::with(|scope| {
        let r = a.re_borrow();
        let (head, tail) = r.head_tail_with(scope);
        (*head, tail)
    });
    drop(head);
    let r = tail.collect_vec();

    a.set(5);
    a.set(7);

    assert_eq!(head, 2);
    assert_eq!(r.stop(), vec![5, 7]);
}
