use reactive_fn::*;

#[test]
fn test_obs_display_map_str() {
    let s = ObsCell::new(1);
    let d = obs_display({
        let s = s.clone();
        move |f, cx| write!(f, "abc{}", s.get(cx))
    });
    let v = d.map_str().map(|x| x.to_string()).collect_vec();
    s.set(5);
    s.set(10);
    let r = v.stop();
    let e = vec!["abc1", "abc5", "abc10"];
    assert_eq!(&r, &e);
}
#[test]
fn test_obs_display_map_string() {
    let s = ObsCell::new(1);
    let d = obs_display({
        let s = s.clone();
        move |f, cx| write!(f, "abc{}", s.get(cx))
    });
    let v = d.map_string().collect_vec();
    s.set(5);
    s.set(10);
    assert_eq!(v.stop(), vec!["abc1", "abc5", "abc10"]);
}

#[test]
fn test_obs_write_constant() {
    let o = obs_display(move |f, cx| obs_write!(f, cx, "abc{}", 10));
    let v = o.map_string().collect_vec();
    assert_eq!(v.stop(), vec!["abc10"]);
}

#[test]
fn test_obs_write_constant_ref() {
    let o = obs_display(move |f, cx| obs_write!(f, cx, "abc{}", &10));
    let v = o.map_string().collect_vec();
    assert_eq!(v.stop(), vec!["abc10"]);
}

#[test]
fn test_obs_write_obs() {
    let s = ObsCell::new(1);
    let o = obs_display({
        let s = s.clone();
        move |f, cx| obs_write!(f, cx, "abc{}", s)
    });
    let v = o.map_string().collect_vec();
    s.set(5);
    s.set(10);
    let r = v.stop();
    let e = vec!["abc1", "abc5", "abc10"];
    assert_eq!(&r, &e);
}
#[test]
fn test_obs_write_obs2() {
    let s0 = ObsCell::new(0);
    let s1 = ObsCell::new(1);
    let o = obs_display({
        let s0 = s0.clone();
        let s1 = s1.clone();
        move |f, cx| obs_write!(f, cx, "abc{}-{}", s0, s1)
    });
    let v = o.map_string().collect_vec();
    s0.set(5);
    s1.set(10);
    let r = v.stop();
    let e = vec!["abc0-1", "abc5-1", "abc5-10"];
    assert_eq!(&r, &e);
}

#[test]
fn test_obs_format_obs() {
    let s = ObsCell::new(1);
    let o = obs({
        let s = s.clone();
        move |cx| obs_format!(cx, "abc{}", s)
    });
    let v = o.collect_vec();
    s.set(5);
    s.set(10);
    let r = v.stop();
    let e = vec!["abc1", "abc5", "abc10"];
    assert_eq!(&r, &e);
}

#[test]
fn test_obs_format_debug() {
    let s = ObsCell::new(Some(1));
    let o = obs({
        let s = s.clone();
        move |cx| obs_format!(cx, "abc-{:?}", s)
    });
    let v = o.collect_vec();
    s.set(None);
    s.set(Some(5));
    let r = v.stop();
    let e = vec!["abc-Some(1)", "abc-None", "abc-Some(5)"];
    assert_eq!(&r, &e);
}
