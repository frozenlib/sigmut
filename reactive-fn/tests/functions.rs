use reactive_fn::*;
use rt_local::runtime::core::test;
use rt_local::wait_for_idle;

#[test]
async fn test_obs_with() {
    let cell = ObsCell::new(1);
    let cell_moved = cell.clone();
    let o = obs_with(move |oc| oc.ret_flat(&cell_moved));
    let d = o.into_dyn();
    let r = d.collect_vec();
    wait_for_idle().await;
    cell.set(5);
    wait_for_idle().await;
    cell.set(10);
    wait_for_idle().await;
    let r = r.stop();
    assert_eq!(r, [1, 5, 10]);
}
