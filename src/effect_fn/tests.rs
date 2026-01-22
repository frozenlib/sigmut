use assert_call::{CallRecorder, call};

use crate::{ReactionPhase, Signal, State, core::Runtime, effect, effect_in};

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
fn test_effect_in() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let phase_1 = ReactionPhase::new(1);
    let phase_2 = ReactionPhase::new(2);

    let s = Signal::from_value(10);
    let _e = effect_in(move |sc| call!("{}", s.get(sc)), phase_1);
    rt.dispatch_reactions(phase_2);
    cr.verify(());

    rt.dispatch_reactions(phase_1);
    cr.verify("10");
}
