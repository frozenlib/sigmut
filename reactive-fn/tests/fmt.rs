use std::cell::RefCell;

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
    let r = v.stop();
    let e = vec!["abc1", "abc5", "abc10"];
    assert_eq!(&r, &e);
}

#[test]
fn test_to_format_arg() {
    let s = ObsCell::new(1);
    let d = s.obs().into_obs_display();
    let o = obs(move |cx| format!("abc{}", d.to_format_arg(&RefCell::new(cx))));
    let v = o.collect_vec();
    s.set(5);
    s.set(10);
    let r = v.stop();
    let e = vec!["abc1", "abc5", "abc10"];
    assert_eq!(&r, &e);
}
#[test]
fn test_to_format_arg2() {
    let s0 = ObsCell::new(1);
    let s1 = ObsCell::new(1);
    let d0 = s0.obs().into_obs_display();
    let d1 = s1.obs().into_obs_display();
    let o = obs(move |cx| {
        let cx = RefCell::new(cx);
        format!("abc{}-{}", d0.to_format_arg(&cx), d1.to_format_arg(&cx))
    });
    let v = o.collect_vec();
    s0.set(5);
    s1.set(10);
    let r = v.stop();
    let e = vec!["abc1-1", "abc5-1", "abc5-10"];
    assert_eq!(&r, &e);
}

#[test]
fn test_obs_write_constant() {
    let o = obs_display(move |f, cx| obs_write!(f, cx, "abc{}", 10));
    let v = o.map_string().collect_vec();
    let r = v.stop();
    let e = vec!["abc10"];
    assert_eq!(&r, &e);
}

#[test]
fn test_obs_write_constant_ref() {
    let o = obs_display(move |f, cx| obs_write!(f, cx, "abc{}", &10));
    let v = o.map_string().collect_vec();
    let r = v.stop();
    let e = vec!["abc10"];
    assert_eq!(&r, &e);
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
fn test_obs_write_obs_by_ref() {
    let s = ObsCell::new(1);
    let o = obs_display({
        let s = s.clone();
        move |f, cx| obs_write!(f, cx, "abc{}", &s)
    });
    let v = o.map_string().collect_vec();
    s.set(5);
    s.set(10);
    let r = v.stop();
    let e = vec!["abc1", "abc5", "abc10"];
    assert_eq!(&r, &e);
}
