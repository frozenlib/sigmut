use std::future::pending;

use assert_call::{CallRecorder, call};

use crate::{State, core::Runtime, effect_async, utils::test_helpers::call_on_drop};

#[test]
fn completed_future_is_dropped_outside_node_borrow() {
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    struct ReadyWithDrop(Box<dyn Fn()>);
    impl std::future::Future for ReadyWithDrop {
        type Output = ();
        fn poll(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<()> {
            std::task::Poll::Ready(())
        }
    }
    impl Drop for ReadyWithDrop {
        fn drop(&mut self) {
            (self.0)();
        }
    }
    let check = Rc::new(RefCell::new(None::<Box<dyn Fn()>>));
    let outside = Rc::new(Cell::new(false));
    let node = super::EffectAsyncNode::new(
        {
            let check = check.clone();
            move |_| {
                let check = check.clone();
                ReadyWithDrop(Box::new(move || (check.borrow().as_ref().unwrap())()))
            }
        },
        crate::ReactionPhase::DEFAULT,
    );
    *check.borrow_mut() = Some(Box::new({
        let node = Rc::downgrade(&node);
        let outside = outside.clone();
        move || {
            outside.set(node.upgrade().unwrap().data.try_borrow_mut().is_ok());
        }
    }));
    let mut runtime = Runtime::new();
    node.call(&mut runtime.rc());
    assert!(outside.get());
}

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
