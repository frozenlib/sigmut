use reactive_fn::*;

#[test]
fn re_cell_dyn() {
    let cell = ReCell::new(1);
    let r = cell.re().collect_vec();
    cell.set(5);
    cell.set(10);

    assert_eq!(r.stop(), vec![1, 5, 10]);
}

#[test]
fn re_cell() {
    let cell = ReCell::new(1);
    let r = cell.ops().collect_vec();
    cell.set(5);
    cell.set(10);

    assert_eq!(r.stop(), vec![1, 5, 10]);
}

#[test]
fn re_ref_cell_dyn() {
    let cell = ReRefCell::new(1);
    let r = cell.re_borrow().collect_vec();
    cell.set(5);
    cell.set(10);

    assert_eq!(r.stop(), vec![1, 5, 10]);
}

#[test]
fn re_ref_cell() {
    let cell = ReRefCell::new(1);
    let r = cell.ops().collect_vec();
    cell.set(5);
    cell.set(10);

    assert_eq!(r.stop(), vec![1, 5, 10]);
}