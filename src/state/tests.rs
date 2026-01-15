use assert_call::{CallRecorder, call};

use crate::{State, core::Runtime, effect};

#[test]
fn new() {
    let mut rt = Runtime::new();
    let s = State::new(10);
    assert_eq!(s.get(&mut rt.sc()), 10);
}

#[test]
fn set() {
    let mut rt = Runtime::new();
    let s = State::new(10);
    assert_eq!(s.get(&mut rt.sc()), 10);

    s.set(20, rt.ac());
    assert_eq!(s.get(&mut rt.sc()), 20);

    s.set(30, rt.ac());
    assert_eq!(s.get(&mut rt.sc()), 30);
}

#[test]
fn set_effect() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();
    let s = State::new(10);
    let s0 = s.clone();
    let _e = effect(move |sc| {
        call!("{}", s0.get(sc));
    });
    cr.verify(());
    rt.flush();
    cr.verify("10");

    s.set(20, rt.ac());
    cr.verify(());
    rt.flush();
    cr.verify("20");

    s.set(30, rt.ac());
    s.set(40, rt.ac());
    rt.flush();
    cr.verify("40");
}

#[test]
fn set_dedup_effect() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();
    let s = State::new(10);
    let s0 = s.clone();
    let _e = effect(move |sc| {
        call!("{}", s0.get(sc));
    });

    cr.verify(());
    rt.flush();
    cr.verify("10");

    s.set(10, rt.ac());
    rt.flush();
    cr.verify("10");

    s.set_dedup(10, rt.ac());
    rt.flush();
    cr.verify(());

    s.set_dedup(20, rt.ac());
    rt.flush();
    cr.verify("20");
}

#[test]
fn borrow_mut() {
    let mut rt = Runtime::new();
    let s = State::new(10);
    assert_eq!(s.get(&mut rt.sc()), 10);

    {
        let mut borrowed = s.borrow_mut(rt.ac());
        *borrowed = 20;
    }
    rt.flush();
    assert_eq!(s.get(&mut rt.sc()), 20);
}

#[test]
fn borrow_mut_loose() {
    let mut rt = Runtime::new();
    let s1 = State::new(10);
    let s2 = State::new(20);

    {
        let mut b1 = s1.borrow_mut_loose(rt.ac());
        let mut b2 = s2.borrow_mut_loose(rt.ac());
        *b1 = 30;
        *b2 = 40;
    }
    rt.flush();

    assert_eq!(s1.get(&mut rt.sc()), 30);
    assert_eq!(s2.get(&mut rt.sc()), 40);
}

#[test]
fn borrow_mut_dedup() {
    let mut rt = Runtime::new();
    let s = State::new(10);
    let mut cr = CallRecorder::new();
    let s0 = s.clone();
    let _e = effect(move |sc| {
        call!("{}", s0.get(sc));
    });
    cr.verify(());
    rt.flush();
    cr.verify("10");

    {
        let mut borrowed = s.borrow_mut_dedup(rt.ac());
        *borrowed = 10;
    }
    rt.flush();
    cr.verify(());

    {
        let mut borrowed = s.borrow_mut_dedup(rt.ac());
        *borrowed = 20;
    }
    rt.flush();
    cr.verify("20");
}

#[test]
fn borrow_mut_dedup_loose() {
    let mut rt = Runtime::new();
    let s1 = State::new(10);
    let s2 = State::new(20);

    {
        let mut b1 = s1.borrow_mut_dedup_loose(rt.ac());
        let mut b2 = s2.borrow_mut_dedup_loose(rt.ac());
        *b1 = 30;
        *b2 = 40;
    }
    rt.flush();

    assert_eq!(s1.get(&mut rt.sc()), 30);
    assert_eq!(s2.get(&mut rt.sc()), 40);
}

#[test]
fn debug_implementation() {
    let s = State::new(42);
    let debug_str = format!("{s:?}");
    assert_eq!(debug_str, "42");

    let s = State::new("hello");
    let debug_str = format!("{s:?}");
    assert_eq!(debug_str, "\"hello\"");
}

#[test]
fn serialize_deserialize() {
    use serde_json;
    let s = State::new(42);
    let serialized = serde_json::to_string(&s).unwrap();
    assert_eq!(serialized, "42");

    let deserialized: State<i32> = serde_json::from_str(&serialized).unwrap();
    let mut rt = Runtime::new();
    assert_eq!(deserialized.get(&mut rt.sc()), 42);
}
