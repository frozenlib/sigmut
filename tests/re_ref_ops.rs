use reactive_fn::*;

#[test]
fn re_ref_constant_test() {
    let r = re_ref_constant(2).to_vec();
    assert_eq!(r.stop(), vec![2]);
}
#[test]
fn re_ref_new() {
    let a = ReCell::new(2);
    let r = ReRef::new(a.clone(), move |a, ctx, f| {
        let value = a.get(ctx);
        f(ctx, &value)
    })
    .to_vec();

    a.set_and_update(5);
    a.set_and_update(7);

    assert_eq!(r.stop(), vec![2, 5, 7]);
}

#[test]
fn re_ref_new_cell2() {
    let cell1 = ReCell::new(1);
    let cell2 = ReCell::new(2);

    let r = ReRef::new(
        (cell1.clone(), cell2.clone()),
        move |(cell1, cell2), ctx, f| {
            let value = cell1.get(ctx) + cell2.get(ctx);
            f(ctx, &value)
        },
    )
    .to_vec();

    cell1.set_and_update(5);
    cell2.set_and_update(10);

    assert_eq!(r.stop(), vec![1 + 2, 5 + 2, 5 + 10]);
}

#[test]
fn re_ref_map() {
    let a = ReRefCell::new(2);
    let r = a.to_re_ref().map(|x| x * 2).to_vec();

    a.set_and_update(5);
    a.set_and_update(7);

    assert_eq!(r.stop(), vec![4, 10, 14]);
}

#[test]
fn re_ref_map_ref() {
    let a = ReRefCell::new((2, 3));
    let r = a.to_re_ref().map_ref(|x| &x.0).to_vec();

    a.set_and_update((5, 8));
    a.set_and_update((7, 1));

    assert_eq!(r.stop(), vec![2, 5, 7]);
}
#[test]
fn re_ref_flat_map() {
    let a = [ReCell::new(5), ReCell::new(10)];
    let a_ = a.clone();

    let b = ReRefCell::new(0);

    let r = b.to_re_ref().flat_map(move |&x| a_[x].to_re()).to_vec();

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
fn re_ref_cloned() {
    let cell = ReRefCell::new(2);
    let r = cell.to_re_ref().cloned().to_vec();

    cell.set_and_update(3);
    cell.set_and_update(4);
    cell.set_and_update(5);

    assert_eq!(r.stop(), vec![2, 3, 4, 5]);
}

#[test]
fn re_ref_scan() {
    let cell = ReRefCell::new(2);
    let r = cell.to_re_ref().scan(10, |s, x| s + x).to_vec();

    cell.set_and_update(3);
    cell.set_and_update(4);
    cell.set_and_update(5);

    assert_eq!(r.stop(), vec![12, 15, 19, 24]);
}
#[test]
fn re_ref_filter_scan() {
    let cell = ReRefCell::new(2);
    let r = cell
        .to_re_ref()
        .filter_scan(10, |_s, x| x % 2 != 0, |s, x| s + x)
        .to_vec();

    cell.set_and_update(3);
    cell.set_and_update(4);
    cell.set_and_update(5);
    cell.set_and_update(6);

    assert_eq!(r.stop(), vec![10, 13, 18]);
}
#[test]
fn re_ref_fold() {
    let cell = ReRefCell::new(1);
    let fold = cell.to_re_ref().fold(2, |s, x| s + x);

    cell.set_and_update(5);
    cell.set_and_update(10);

    assert_eq!(fold.stop(), 18);
}

#[test]
fn re_ref_to_vec() {
    let cell = ReRefCell::new(1);
    let fold = cell.to_re_ref().to_vec();

    cell.set_and_update(2);
    cell.set_and_update(1);
    cell.set_and_update(3);

    assert_eq!(fold.stop(), vec![1, 2, 1, 3]);
}

#[test]
fn re_ref_for_each() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let cell = ReRefCell::new(0);
    let vs = Rc::new(RefCell::new(Vec::new()));

    let vs_send = vs.clone();

    let r = cell.to_re_ref().for_each(move |&x| {
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
fn re_ref_hot() {
    let cell = ReRefCell::new(1);
    let re = cell.to_re_ref().scan(0, |s, x| s + x);

    let hot = re.hot();

    cell.set_and_update(2);
    cell.set_and_update(10);

    assert_eq!(hot.to_vec().stop(), vec![13]);
}

#[test]
fn re_ref_hot_no() {
    let cell = ReRefCell::new(1);
    let re = cell.to_re_ref().scan(0, |s, x| s + x);

    cell.set_and_update(2);
    cell.set_and_update(10);

    assert_eq!(re.to_vec().stop(), vec![10]);
}

#[test]
fn re_ref_flatten() {
    let cell = ReRefCell::new(Re::constant(1));

    let vs = cell.to_re_ref().flatten().to_vec();

    cell.set_and_update(Re::constant(2));
    cell.set_and_update(Re::constant(3));
    cell.set_and_update(Re::constant(4));
    cell.set_and_update(Re::constant(5));

    assert_eq!(vs.stop(), vec![1, 2, 3, 4, 5]);
}

#[test]
fn re_ref_head_tail() {
    let a = ReRefCell::new(2);
    let mut head = None;
    let tail = BindContextScope::with(|scope| {
        a.to_re_ref().head_tail(scope, |&value| {
            head = Some(value);
        })
    });
    let r = tail.to_vec();

    a.set_and_update(5);
    a.set_and_update(7);

    assert_eq!(head, Some(2));
    assert_eq!(r.stop(), vec![5, 7]);
}