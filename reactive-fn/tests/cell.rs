use reactive_fn::*;

#[test]
fn obs_cell_dyn() {
    let cell = ObsCell::new(1);
    let r = cell.as_dyn().collect_vec();
    cell.set(5);
    cell.set(10);

    assert_eq!(r.stop(), vec![1, 5, 10]);
}

#[test]
fn obs_cell() {
    let cell = ObsCell::new(1);
    let r = cell.obs().collect_vec();
    cell.set(5);
    cell.set(10);

    assert_eq!(r.stop(), vec![1, 5, 10]);
}

#[test]
fn obs_ref_cell_dyn() {
    let cell = ObsCell::new(1);
    let r = cell.as_dyn().collect_vec();
    cell.set(5);
    cell.set(10);

    assert_eq!(r.stop(), vec![1, 5, 10]);
}

#[test]
fn obs_ref_cell() {
    let cell = ObsCell::new(1);
    let r = cell.obs().collect_vec();
    cell.set(5);
    cell.set(10);

    assert_eq!(r.stop(), vec![1, 5, 10]);
}

#[test]
fn serailize() {
    let c0 = ObsCell::new(1);
    let bytes = bincode::serialize(&c0).expect("failed to serialize.");
    let c1: ObsCell<u8> = bincode::deserialize(&bytes).expect("failed to deserialize.");
    assert_eq!(c1.get_head(), c0.get_head());
}
