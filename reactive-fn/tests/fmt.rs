use ::rt_local;
use ::rt_local::yield_now;
use reactive_fn::*;

#[rt_local::test]
async fn impl_observable_display() {
    fn check(_s: impl ObservableDisplay) {}

    let s = ObsCell::new(0);
    let s_dyn = s.obs().map_ref(|x| x as &dyn ObservableDisplay).into_dyn();
    check(s_dyn);
}

#[rt_local::test]
async fn test_obs_display_map_str() {
    let s = ObsCell::new(1);
    let d = obs_display({
        let s = s.clone();
        move |f, bc| write!(f, "abc{}", s.get(bc))
    });
    let v = d.obs().map(|x| x.to_string()).collect_vec();
    yield_now().await;

    s.set(5);
    yield_now().await;

    s.set(10);
    yield_now().await;

    assert_eq!(v.stop(), vec!["abc1", "abc5", "abc10"]);
}
#[rt_local::test]
async fn test_obs_display_map_string() {
    let s = ObsCell::new(1);
    let d = obs_display({
        let s = s.clone();
        move |f, bc| write!(f, "abc{}", s.get(bc))
    });
    let v = d.obs().collect_vec();
    yield_now().await;

    s.set(5);
    yield_now().await;

    s.set(10);
    yield_now().await;

    assert_eq!(v.stop(), vec!["abc1", "abc5", "abc10"]);
}

#[rt_local::test]
async fn test_bind_write_constant() {
    let o = obs_display(move |f, bc| bind_write!(f, bc, "abc{}", 10));
    let v = o.obs().collect_vec();
    yield_now().await;
    assert_eq!(v.stop(), vec!["abc10"]);
}

#[rt_local::test]
async fn test_bind_write_constant_ref() {
    let o = obs_display(move |f, bc| bind_write!(f, bc, "abc{}", &10));
    let v = o.obs().collect_vec();
    yield_now().await;
    assert_eq!(v.stop(), vec!["abc10"]);
}

#[rt_local::test]
async fn test_bind_write_obs() {
    let s = ObsCell::new(1);
    let o = obs_display({
        let s = s.clone();
        move |f, bc| bind_write!(f, bc, "abc{}", s)
    });
    let v = o.obs().collect_vec();
    yield_now().await;

    s.set(5);
    yield_now().await;

    s.set(10);
    yield_now().await;

    assert_eq!(v.stop(), vec!["abc1", "abc5", "abc10"]);
}
#[rt_local::test]
async fn test_bind_write_obs2() {
    let s0 = ObsCell::new(0);
    let s1 = ObsCell::new(1);
    let o = obs_display({
        let s0 = s0.clone();
        let s1 = s1.clone();
        move |f, bc| bind_write!(f, bc, "abc{}-{}", s0, s1)
    });
    let v = o.obs().collect_vec();
    yield_now().await;

    s0.set(5);
    yield_now().await;

    s1.set(10);
    yield_now().await;

    assert_eq!(v.stop(), vec!["abc0-1", "abc5-1", "abc5-10"]);
}

#[rt_local::test]
async fn test_bind_write_obs_format() {
    let s = ObsCell::new(1);
    let o = obs_display({
        let s = s.clone();
        move |f, bc| bind_write!(f, bc, "abc-{}", obs_format!("<{}>", s.clone()))
    });
    let v = o.obs().collect_vec();
    yield_now().await;

    s.set(5);
    yield_now().await;

    s.set(10);
    yield_now().await;

    assert_eq!(v.stop(), vec!["abc-<1>", "abc-<5>", "abc-<10>"]);
}

#[rt_local::test]
async fn test_bind_format_obs() {
    let s = ObsCell::new(1);
    let o = obs({
        let s = s.clone();
        move |bc| bind_format!(bc, "abc{}", s)
    });
    let v = o.collect_vec();
    yield_now().await;

    s.set(5);
    yield_now().await;

    s.set(10);
    yield_now().await;

    assert_eq!(v.stop(), vec!["abc1", "abc5", "abc10"]);
}

#[rt_local::test]
async fn test_bind_format_name() {
    let s0 = ObsCell::new(1);
    let s1 = ObsCell::new(5);
    let o = obs({
        let s0 = s0.clone();
        let s1 = s1.clone();
        move |bc| bind_format!(bc, "{abc}-{def}", def = s0, abc = s1)
    });
    let v = o.collect_vec();
    yield_now().await;

    s0.set(7);
    yield_now().await;

    s1.set(10);
    yield_now().await;

    assert_eq!(v.stop(), vec!["5-1", "5-7", "10-7"]);
}

#[rt_local::test]
async fn test_bind_format_debug() {
    let s = ObsCell::new(Some(1));
    let o = obs({
        let s = s.clone();
        move |bc| bind_format!(bc, "abc-{:?}", s)
    });
    let v = o.collect_vec();
    yield_now().await;

    s.set(None);
    yield_now().await;

    s.set(Some(5));
    yield_now().await;

    assert_eq!(v.stop(), vec!["abc-Some(1)", "abc-None", "abc-Some(5)"]);
}

#[rt_local::test]
async fn test_bind_format_hex() {
    let s = ObsCell::new(10);
    let o = obs({
        let s = s.clone();
        move |bc| bind_format!(bc, "abc-{:x}", s)
    });
    let v = o.collect_vec();
    yield_now().await;

    s.set(16);
    yield_now().await;

    s.set(20);
    yield_now().await;

    assert_eq!(v.stop(), vec!["abc-a", "abc-10", "abc-14"]);
}

#[rt_local::test]
async fn test_obs_format() {
    let s = ObsCell::new(10);
    let v = obs_format!("abc-{}", s.clone()).obs().collect_vec();
    yield_now().await;

    s.set(16);
    yield_now().await;

    s.set(20);
    yield_now().await;

    assert_eq!(v.stop(), vec!["abc-10", "abc-16", "abc-20"]);
}

#[rt_local::test]
async fn test_obs_format_obs_display() {
    let s = ObsCell::new(10);
    let d = s.obs().display();
    let v = obs_format!("abc-{}", d).obs().collect_vec();
    yield_now().await;

    s.set(16);
    yield_now().await;

    s.set(20);
    yield_now().await;

    assert_eq!(v.stop(), vec!["abc-10", "abc-16", "abc-20"]);
}
#[rt_local::test]
async fn test_obs_format_dyn_obs_dyn_obs_display() {
    let s = ObsCell::new(10);
    let d = s.as_dyn().map_ref(|x| x as &dyn ObservableDisplay);
    let v = obs_format!("abc-{}", d).obs().collect_vec();
    yield_now().await;

    s.set(16);
    yield_now().await;

    s.set(20);
    yield_now().await;

    assert_eq!(v.stop(), vec!["abc-10", "abc-16", "abc-20"]);
}

#[rt_local::test]
async fn test_obs_format_hex() {
    let v = obs_format!("abc-{:x}", 0xa0).obs().collect_vec();
    yield_now().await;
    assert_eq!(v.stop(), vec!["abc-a0"]);
}

#[rt_local::test]
async fn test_obs_format_obs_hex() {
    let s = ObsCell::new(0xff);
    let v = obs_format!("abc-{:x}", s.obs()).obs().collect_vec();
    yield_now().await;

    s.set(0x10);
    yield_now().await;

    s.set(0x20);
    yield_now().await;

    assert_eq!(v.stop(), vec!["abc-ff", "abc-10", "abc-20"]);
}
