use reactive_fn::*;

#[test]
fn re_cell_dyn() {
    let cell = ObsCell::new(1);
    let r = cell.as_dyn().collect_vec();
    cell.set(5);
    cell.set(10);

    assert_eq!(r.stop(), vec![1, 5, 10]);
}

#[test]
fn re_cell() {
    let cell = ObsCell::new(1);
    let r = cell.obs().collect_vec();
    cell.set(5);
    cell.set(10);

    assert_eq!(r.stop(), vec![1, 5, 10]);
}

#[test]
fn re_ref_cell_dyn() {
    let cell = ObsRefCell::new(1);
    let r = cell.as_dyn().collect_vec();
    cell.set(5);
    cell.set(10);

    assert_eq!(r.stop(), vec![1, 5, 10]);
}

#[test]
fn re_ref_cell() {
    let cell = ObsRefCell::new(1);
    let r = cell.obs().collect_vec();
    cell.set(5);
    cell.set(10);

    assert_eq!(r.stop(), vec![1, 5, 10]);
}
