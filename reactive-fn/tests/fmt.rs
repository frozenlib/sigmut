use reactive_fn::{
    core::Runtime, obs_format, observable::ObsCell, watch_format, watch_write, watch_writeln,
};
use std::fmt::Write;

struct DebugOnly(u32);
impl std::fmt::Debug for DebugOnly {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "debug-{}", self.0)
    }
}

#[test]
fn watch_write() {
    Runtime::with(|dc| {
        let mut s = String::new();
        let _ = watch_write!(&mut s, dc.ac().oc(), "{}", 1 + 2);
        assert_eq!(s, "3");
    });
}

#[test]
fn watch_writeln() {
    Runtime::with(|dc| {
        let mut s = String::new();
        let _ = watch_writeln!(&mut s, dc.ac().oc(), "{}", 1 + 2);
        assert_eq!(s, "3\n");
    });
}

#[test]
fn watch_format() {
    Runtime::with(|dc| {
        let s = watch_format!(dc.ac().oc(), "{}", 1 + 2);
        assert_eq!(s, "3");
    });
}

#[test]
fn watch_write_display() {
    Runtime::with(|dc| {
        let mut s = String::new();
        let _ = watch_write!(&mut s, dc.ac().oc(), "{}", 1 + 2);
        assert_eq!(s, "3");
    });
}

#[test]
fn obs_format_display() {
    Runtime::with(|dc| {
        let s = obs_format!("{}", 1 + 2);
        assert_eq!(s.get(dc.ac().oc()), "3");
    });
}

#[test]
fn watch_write_observable_display() {
    Runtime::with(|dc| {
        let mut s = String::new();
        let a = ObsCell::new(1);
        let _ = watch_write!(&mut s, dc.ac().oc(), "{a}");
        assert_eq!(s, "1");
    });
}
#[test]
fn obs_format_observable_display() {
    Runtime::with(|dc| {
        let a = ObsCell::new(3);
        let s = obs_format!("{a}");
        assert_eq!(s.get(dc.ac().oc()), "3");
    });
}

#[test]
fn obs_format_observable_display_notify() {
    Runtime::with(|dc| {
        let a = ObsCell::new(3);
        let s = obs_format!("{}", a.obs());
        let sc = s.cached();
        assert_eq!(sc.get(dc.ac().oc()), "3");

        a.set(5, &mut dc.ac());
        assert_eq!(sc.get(dc.ac().oc()), "5");
    });
}

#[test]
fn watch_write_debug() {
    Runtime::with(|dc| {
        let mut s = String::new();
        let _ = watch_write!(&mut s, dc.ac().oc(), "{:?}", DebugOnly(3));
        assert_eq!(s, "debug-3");
    });
}
#[test]
fn obs_format_debug() {
    Runtime::with(|dc| {
        let s = obs_format!("{:?}", DebugOnly(3));
        assert_eq!(s.get(dc.ac().oc()), "debug-3");
    });
}

#[test]
fn watch_write_index() {
    Runtime::with(|dc| {
        let mut s = String::new();
        let _ = watch_write!(&mut s, dc.ac().oc(), "{1} {0}", 1 + 2, 3 + 4);
        assert_eq!(s, "7 3");
    });
}
#[test]
fn obs_format_index() {
    Runtime::with(|dc| {
        let s = obs_format!("{1} {0}", 1 + 2, 3 + 4);
        assert_eq!(s.get(dc.ac().oc()), "7 3");
    });
}

#[test]
fn watch_write_key_value() {
    Runtime::with(|dc| {
        let mut s = String::new();
        let _ = watch_write!(&mut s, dc.ac().oc(), "{a}", a = 1 + 2);
        assert_eq!(s, "3");
    });
}
#[test]
fn obs_format_key_value() {
    Runtime::with(|dc| {
        let s = obs_format!("{a}", a = 1 + 2);
        assert_eq!(s.get(dc.ac().oc()), "3");
    });
}

#[test]
fn watch_write_key_value_2() {
    Runtime::with(|dc| {
        let mut s = String::new();
        let _ = watch_write!(&mut s, dc.ac().oc(), "{b} {a}", a = 1 + 2, b = 5);
        assert_eq!(s, "5 3");
    });
}
#[test]
fn obs_format_key_value_2() {
    Runtime::with(|dc| {
        let s = obs_format!("{b} {a}", a = 1 + 2, b = 5);
        assert_eq!(s.get(dc.ac().oc()), "5 3");
    });
}

#[test]
fn watch_write_key() {
    Runtime::with(|dc| {
        let mut s = String::new();
        let a = 3;
        let b = 5;
        let _ = watch_write!(&mut s, dc.ac().oc(), "{b} {a}");
        assert_eq!(s, "5 3");
    });
}

#[test]
fn obs_format_key() {
    Runtime::with(|dc| {
        let a = ObsCell::new(3);
        let b = ObsCell::new(5);
        let s = obs_format!("{b} {a}");
        assert_eq!(s.get(dc.ac().oc()), "5 3");
    });
}

#[test]
fn watch_write_mix() {
    Runtime::with(|dc| {
        let mut s = String::new();
        let a = 1;
        let _ = watch_write!(&mut s, dc.ac().oc(), "{a} {x} {}", 5, x = 9);
        assert_eq!(s, "1 9 5");
    });
}

#[test]
fn obs_format_mix() {
    Runtime::with(|dc| {
        let a = ObsCell::new(1);
        let x = ObsCell::new(9);
        let s = obs_format!("{a} {x} {}", 5);
        assert_eq!(s.get(dc.ac().oc()), "1 9 5");
    });
}

#[test]
fn watch_not_consume() {
    Runtime::with(|dc| {
        let mut s = String::new();
        let a = String::from("abc");
        let _ = watch_write!(&mut s, dc.ac().oc(), "{a}");
        let _ = watch_write!(&mut s, dc.ac().oc(), "{a}");
        assert_eq!(s, "abcabc");
    });
}

#[test]
fn watch_write_escape() {
    Runtime::with(|dc| {
        let mut s = String::new();
        #[allow(unused)]
        let a = 3;
        let _ = watch_write!(&mut s, dc.ac().oc(), "{{a}}");
        assert_eq!(s, "{a}");
    });
}
