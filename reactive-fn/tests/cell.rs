use ::rt_local::runtime::core::test;
use ::rt_local::wait_for_idle;
use reactive_fn::*;

#[test]
async fn obs_cell_dyn() {
    let cell = ObsCell::new(1);
    let r = cell.as_dyn().collect_vec();
    wait_for_idle().await;

    cell.set(5);
    wait_for_idle().await;

    cell.set(10);
    wait_for_idle().await;

    assert_eq!(r.stop(), vec![1, 5, 10]);
}

#[test]
async fn obs_cell() {
    let cell = ObsCell::new(1);
    let r = cell.obs().collect_vec();
    wait_for_idle().await;

    cell.set(5);
    wait_for_idle().await;

    cell.set(10);
    wait_for_idle().await;

    assert_eq!(r.stop(), vec![1, 5, 10]);
}

#[test]
async fn obs_ref_cell_dyn() {
    let cell = ObsCell::new(1);
    let r = cell.as_dyn().collect_vec();
    wait_for_idle().await;

    cell.set(5);
    wait_for_idle().await;

    cell.set(10);
    wait_for_idle().await;

    assert_eq!(r.stop(), vec![1, 5, 10]);
}

#[test]
async fn obs_ref_cell() {
    let cell = ObsCell::new(1);
    let r = cell.obs().collect_vec();
    wait_for_idle().await;

    cell.set(5);
    wait_for_idle().await;

    cell.set(10);
    wait_for_idle().await;

    assert_eq!(r.stop(), vec![1, 5, 10]);
}

#[test]
async fn serailize() {
    let c0 = ObsCell::new(1);
    let bytes = bincode::serialize(&c0).expect("failed to serialize.");
    let c1: ObsCell<u8> = bincode::deserialize(&bytes).expect("failed to deserialize.");
    assert_eq!(c1.get_head(), c0.get_head());
}
