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
