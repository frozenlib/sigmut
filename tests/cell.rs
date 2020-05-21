use reactive_fn::*;

#[test]
fn re_cell() {
    let cell = ReCell::new(1);
    let r = cell.to_re().to_vec();
    cell.set_and_update(5);
    cell.set_and_update(10);

    assert_eq!(r.stop(), vec![1, 5, 10]);
}

#[test]
fn re_ref_cell() {
    let cell = ReRefCell::new(1);
    let r = cell.to_re_borrow().cloned().to_vec();
    cell.set_and_update(5);
    cell.set_and_update(10);

    assert_eq!(r.stop(), vec![1, 5, 10]);
}
