use ::rt_local;
use ::rt_local::yield_now;
use reactive_fn::*;

#[rt_local::test]
async fn obs_cell_dyn() {
    let cell = ObsCell::new(1);
    let r = cell.as_dyn().collect_vec();
    yield_now().await;

    cell.set(5);
    yield_now().await;

    cell.set(10);
    yield_now().await;

    assert_eq!(r.stop(), vec![1, 5, 10]);
}

#[rt_local::test]
async fn obs_cell() {
    let cell = ObsCell::new(1);
    let r = cell.obs().collect_vec();
    yield_now().await;

    cell.set(5);
    yield_now().await;

    cell.set(10);
    yield_now().await;

    assert_eq!(r.stop(), vec![1, 5, 10]);
}

#[rt_local::test]
async fn obs_ref_cell_dyn() {
    let cell = ObsCell::new(1);
    let r = cell.as_dyn().collect_vec();
    yield_now().await;

    cell.set(5);
    yield_now().await;

    cell.set(10);
    yield_now().await;

    assert_eq!(r.stop(), vec![1, 5, 10]);
}

#[rt_local::test]
async fn obs_ref_cell() {
    let cell = ObsCell::new(1);
    let r = cell.obs().collect_vec();
    yield_now().await;

    cell.set(5);
    yield_now().await;

    cell.set(10);
    yield_now().await;

    assert_eq!(r.stop(), vec![1, 5, 10]);
}

#[rt_local::test]
async fn serailize() {
    let c0 = ObsCell::new(1);
    let bytes = bincode::serialize(&c0).expect("failed to serialize.");
    let c1: ObsCell<u8> = bincode::deserialize(&bytes).expect("failed to deserialize.");
    assert_eq!(c1.get_head(), c0.get_head());
}
