use std::fmt::Display;

use crate::{State, core::Runtime, signal::ToSignal, signal_format};

#[allow(unused)]
use crate::signal_format_dump;

#[test]
fn none() {
    let mut rt = Runtime::new();
    let s = signal_format!("");
    assert_eq!(s.get(&mut rt.sc()), "");
}

#[test]
fn value_display() {
    let mut rt = Runtime::new();
    let s = signal_format!("{}", 1usize);
    assert_eq!(s.get(&mut rt.sc()), "1");
}
#[test]
fn signal_display() {
    let mut rt = Runtime::new();

    let a = State::new(0);
    let s = signal_format!("{}", a.to_signal());
    let sc = &mut rt.sc();
    assert_eq!(s.get(sc), format!("{}", a.get(sc)));

    a.set(2, rt.ac());
    let sc = &mut rt.sc();
    assert_eq!(s.get(sc), format!("{}", a.get(sc)));
}

#[test]
fn value_debug() {
    let mut rt = Runtime::new();
    let s = signal_format!("{:?}", 1);
    assert_eq!(s.get(&mut rt.sc()), format!("{:?}", 1));
}

#[test]
fn signal_debug() {
    let mut rt = Runtime::new();

    let a = State::new(0);
    let s = signal_format!("{:?}", a.to_signal());
    let sc = &mut rt.sc();
    assert_eq!(s.get(sc), format!("{:?}", a.get(sc)));

    a.set(2, rt.ac());
    let sc = &mut rt.sc();
    assert_eq!(s.get(sc), format!("{:?}", a.get(sc)));
}

#[test]
fn value_binary() {
    let mut rt = Runtime::new();
    let s = signal_format!("{:b}", 5usize);
    assert_eq!(s.get(&mut rt.sc()), format!("{:b}", 5usize));
}

#[test]
fn signal_binary() {
    let mut rt = Runtime::new();

    let a = State::new(3);
    let s = signal_format!("{:b}", a.to_signal());
    let sc = &mut rt.sc();
    assert_eq!(s.get(sc), format!("{:b}", a.get(sc)));

    a.set(5, rt.ac());
    let sc = &mut rt.sc();
    assert_eq!(s.get(sc), format!("{:b}", a.get(sc)));
}

#[test]
fn value_lower_exp() {
    let mut rt = Runtime::new();
    let s = signal_format!("{:e}", 1234.5678);
    assert_eq!(s.get(&mut rt.sc()), format!("{:e}", 1234.5678));
}

#[test]
fn signal_lower_exp() {
    let mut rt = Runtime::new();

    let a = State::new(1.23);
    let s = signal_format!("{:e}", a.to_signal());
    let sc = &mut rt.sc();
    assert_eq!(s.get(sc), format!("{:e}", a.get(sc)));

    a.set(4.56, rt.ac());
    let sc = &mut rt.sc();
    assert_eq!(s.get(sc), format!("{:e}", a.get(sc)));
}

#[test]
fn value_upper_exp() {
    let mut rt = Runtime::new();
    let s = signal_format!("{:E}", 1234.5678);
    assert_eq!(s.get(&mut rt.sc()), format!("{:E}", 1234.5678));
}

#[test]
fn signal_upper_exp() {
    let mut rt = Runtime::new();

    let a = State::new(1.23);
    let s = signal_format!("{:E}", a.to_signal());
    let sc = &mut rt.sc();
    assert_eq!(s.get(sc), format!("{:E}", a.get(sc)));

    a.set(4.56, rt.ac());
    let sc = &mut rt.sc();
    assert_eq!(s.get(sc), format!("{:E}", a.get(sc)));
}

#[test]
fn value_octal() {
    let mut rt = Runtime::new();
    let s = signal_format!("{:o}", 8usize);
    assert_eq!(s.get(&mut rt.sc()), format!("{:o}", 8usize));
}

#[test]
fn signal_octal() {
    let mut rt = Runtime::new();

    let a = State::new(10);
    let s = signal_format!("{:o}", a.to_signal());
    let sc = &mut rt.sc();
    assert_eq!(s.get(sc), format!("{:o}", a.get(sc)));

    a.set(16, rt.ac());
    let sc = &mut rt.sc();
    assert_eq!(s.get(sc), format!("{:o}", a.get(sc)));
}

#[test]
fn value_pointer() {
    let mut rt = Runtime::new();
    let p: *const i32 = &1;
    let s = signal_format!("{:p}", p);
    assert_eq!(s.get(&mut rt.sc()), format!("{p:p}"));
}

#[test]
fn signal_pointer() {
    let mut rt = Runtime::new();

    let a = State::<*const i32>::new(&1);
    let s = signal_format!("{:p}", a);
    let sc = &mut rt.sc();
    assert_eq!(s.get(sc), format!("{:p}", a.get(sc)));

    a.set(&2, rt.ac());
    let sc = &mut rt.sc();
    assert_eq!(s.get(sc), format!("{:p}", a.get(sc)));
}

#[test]
fn value_lower_hex() {
    let mut rt = Runtime::new();
    let s = signal_format!("{:x}", 255usize);
    assert_eq!(s.get(&mut rt.sc()), format!("{:x}", 255usize));
}

#[test]
fn signal_lower_hex() {
    let mut rt = Runtime::new();

    let a = State::new(16);
    let s = signal_format!("{:x}", a.to_signal());
    let sc = &mut rt.sc();
    assert_eq!(s.get(sc), format!("{:x}", a.get(sc)));

    a.set(255, rt.ac());
    let sc = &mut rt.sc();
    assert_eq!(s.get(sc), format!("{:x}", a.get(sc)));
}

#[test]
fn value_upper_hex() {
    let mut rt = Runtime::new();
    let s = signal_format!("{:X}", 255usize);
    assert_eq!(s.get(&mut rt.sc()), format!("{:X}", 255usize));
}

#[test]
fn signal_upper_hex() {
    let mut rt = Runtime::new();

    let a = State::new(16);
    let s = signal_format!("{:X}", a.to_signal());
    let sc = &mut rt.sc();
    assert_eq!(s.get(sc), format!("{:X}", a.get(sc)));

    a.set(255, rt.ac());
    let sc = &mut rt.sc();
    assert_eq!(s.get(sc), format!("{:X}", a.get(sc)));
}

#[test]
fn signal_dyn() {
    let mut rt = Runtime::new();

    let a = State::new(16);
    let a = a.to_signal().map(|x| x as &dyn Display);
    let s = signal_format!("{}", a);
    let sc = &mut rt.sc();
    assert_eq!(s.get(sc), "16");
}

#[test]
fn value_and_signal() {
    let mut rt = Runtime::new();

    let st = State::new(0);
    let s = signal_format!("{}-{}", st, 10);
    assert_eq!(s.get(&mut rt.sc()), "0-10");

    st.set(1, rt.ac());
    assert_eq!(s.get(&mut rt.sc()), "1-10");
}

#[test]
fn use_name() {
    let mut rt = Runtime::new();
    let s = signal_format!("{name}", name = "sigmut");
    assert_eq!(s.get(&mut rt.sc()), "sigmut");
}

#[test]
fn use_index() {
    let mut rt = Runtime::new();
    let s = signal_format!("{1}-{0}", "a", "b");
    assert_eq!(s.get(&mut rt.sc()), "b-a");
}

#[test]
fn use_inline() {
    let mut rt = Runtime::new();
    let x = 10;
    let s = signal_format!("{x}");

    assert_eq!(s.get(&mut rt.sc()), "10");
}

#[test]
fn use_expr() {
    let mut rt = Runtime::new();
    let s = signal_format!("{}", 10 + 20);
    assert_eq!(s.get(&mut rt.sc()), "30");
}

#[test]
fn use_format_spec() {
    let mut rt = Runtime::new();
    let s = signal_format!("{:02}", 1);
    assert_eq!(s.get(&mut rt.sc()), "01");
}

#[test]
fn use_format_spec_signal() {
    let mut rt = Runtime::new();
    let st = State::new(1);
    let s = signal_format!("{:02}", st);
    assert_eq!(s.get(&mut rt.sc()), "01");

    st.set(2, rt.ac());
    assert_eq!(s.get(&mut rt.sc()), "02");
}

#[test]
fn use_dyn_value() {
    let mut rt = Runtime::new();
    let x: &dyn Display = &10usize;
    let s = signal_format!("{}", x);
    assert_eq!(s.get(&mut rt.sc()), "10");
}

#[test]
fn use_ref_to_signal() {
    let mut rt = Runtime::new();
    let st = State::new(5);
    let s = signal_format!("{}", &st);
    assert_eq!(s.get(&mut rt.sc()), "5");
}

#[test]
fn use_dyn_to_signal() {
    let mut rt = Runtime::new();
    let st = State::new(5);
    let st_dyn: &dyn ToSignal<Value = i32> = &st;
    let s = signal_format!("{}", st_dyn);
    assert_eq!(s.get(&mut rt.sc()), "5");
}

#[test]
fn escape() {
    let mut rt = Runtime::new();
    let s = signal_format!("{{}}");
    assert_eq!(s.get(&mut rt.sc()), "{}");
}
