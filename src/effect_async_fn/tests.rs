use std::future::pending;

use assert_call::{CallRecorder, call};

use crate::{State, core::Runtime, effect_async, utils::test_helpers::call_on_drop};

#[test]
fn test_effect_async() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();
    let s = State::new(10);

    let e = effect_async({
        let s = s.to_signal();
        async move |sc| call!("{}", sc.with(|sc| s.get(sc)))
    });
    cr.verify(());

    rt.flush();
    cr.verify("10");

    rt.flush();
    cr.verify(()); // not called again because state did not change

    s.set(20, rt.ac());
    rt.flush();
    cr.verify("20"); // called again because state changed

    s.set(30, rt.ac());
    drop(e);
    cr.verify(()); // not called again because effect was dropped
}

#[test]
fn cancel_on_changed() {
    let mut rt = Runtime::new();

    let mut cr = CallRecorder::new();
    let s = State::new(10);

    let _e = effect_async({
        let s = s.to_signal();
        async move |sc| {
            let value = sc.with(|sc| s.get(sc));
            let _on_drop = call_on_drop(format!("drop_{value}"));
            call!("{value}");
            pending::<()>().await;
        }
    });
    cr.verify(());

    rt.flush();
    cr.verify("10");

    rt.flush();
    cr.verify(()); // not called again because state did not change

    s.set(20, rt.ac());
    rt.flush();
    cr.verify(["drop_10", "20"]); // called again because state changed
}

#[test]
fn cancel_on_drop() {
    let mut rt = Runtime::new();

    let mut cr = CallRecorder::new();
    let s = State::new(10);

    let e = effect_async({
        let s = s.to_signal();
        async move |sc| {
            let value = sc.with(|sc| s.get(sc));
            let _on_drop = call_on_drop(format!("drop_{value}"));
            call!("{value}");
            pending::<()>().await;
        }
    });
    cr.verify(());

    rt.flush();
    cr.verify("10");

    drop(e);
    cr.verify("drop_10");
}
