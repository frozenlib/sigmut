use std::fmt::Display;

use crate::{core::Runtime, signal::ToSignal, signal_format, State};

#[allow(unused)]
use crate::signal_format_dump;

#[test]
fn none() {
    let mut rt = Runtime::new();
    let s = signal_format!("");
    assert_eq!(s.get(&mut rt.sc()), "");
}

#[test]
fn value() {
    let mut rt = Runtime::new();
    let s = signal_format!("{}", 1usize);
    assert_eq!(s.get(&mut rt.sc()), "1");
}
#[test]
fn signal() {
    let mut rt = Runtime::new();

    let st = State::new(0);
    let s = signal_format!("{}", st);
    assert_eq!(s.get(&mut rt.sc()), "0");

    st.set(1, rt.ac());
    assert_eq!(s.get(&mut rt.sc()), "1");

    st.set(2, rt.ac());
    assert_eq!(s.get(&mut rt.sc()), "2");
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
