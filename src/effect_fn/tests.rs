use assert_call::{CallRecorder, call};

use crate::{ReactionKind, Signal, State, core::Runtime, effect, effect_with};

#[test]
fn test_effect() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();
    let s = State::new(10);

    let s0 = s.to_signal();
    let e = effect(move |sc| call!("{}", s0.get(sc)));
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
fn test_effect_with() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let kind_1 = ReactionKind::new(1, "1");
    let kind_2 = ReactionKind::new(2, "2");
    Runtime::register_reaction_kind(kind_1);
    Runtime::register_reaction_kind(kind_2);

    let s = Signal::from_value(10);
    let _e = effect_with(move |sc| call!("{}", s.get(sc)), kind_1);
    rt.dispatch_reactions(kind_2);
    cr.verify(());

    rt.dispatch_reactions(kind_1);
    cr.verify("10");
}
