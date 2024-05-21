use assert_call::{call, CallRecorder};

use crate::{core::Runtime, effect, State};

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
    rt.update();
    cr.verify("10");

    s.set(20, rt.ac());
    cr.verify(());
    rt.update();
    cr.verify("20");

    s.set(30, rt.ac());
    s.set(40, rt.ac());
    rt.update();
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
    rt.update();
    cr.verify("10");

    s.set(10, rt.ac());
    rt.update();
    cr.verify("10");

    s.set_dedup(10, rt.ac());
    rt.update();
    cr.verify(());

    s.set_dedup(20, rt.ac());
    rt.update();
    cr.verify("20");
}
