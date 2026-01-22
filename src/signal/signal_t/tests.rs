use std::{cell::RefCell, rc::Rc, task::Poll};

use crate::{
    ReactionPhase, Signal, SignalBuilder, State, StateRef,
    core::Runtime,
    effect,
    utils::{sync::oneshot_broadcast, test_helpers::call_on_drop},
};
use assert_call::{CallRecorder, call};
use futures::StreamExt;
use rt_local::{runtime::core::test, spawn_local, wait_for_idle};

#[test]
fn new() {
    let mut rt = Runtime::new();

    let st = State::new(5);
    let st_ = st.clone();
    let s = Signal::new(move |sc| st_.get(sc));

    assert_eq!(s.get(&mut rt.sc()), 5);

    st.set(10, rt.ac());
    assert_eq!(s.get(&mut rt.sc()), 10);
}

#[test]
fn new_nested() {
    let mut rt = Runtime::new();

    let st = State::new(5);
    let st_ = st.clone();
    let s0 = Signal::new(move |sc| st_.get(sc));
    let s1 = Signal::new(move |sc| s0.get(sc));

    assert_eq!(s1.get(&mut rt.sc()), 5);

    st.set(10, rt.ac());
    assert_eq!(s1.get(&mut rt.sc()), 10);
}

#[test]
fn new_nested_2() {
    let mut rt = Runtime::new();

    let st = State::new(5);
    let st_ = st.clone();
    let s0 = Signal::new(move |sc| st_.get(sc));
    let s1 = Signal::new(move |sc| s0.get(sc));
    let s2 = Signal::new(move |sc| s1.get(sc));

    assert_eq!(s2.get(&mut rt.sc()), 5);

    st.set(10, rt.ac());
    assert_eq!(s2.get(&mut rt.sc()), 10);
}

#[test]
fn new_nested_3() {
    let mut rt = Runtime::new();

    let st = State::new(5);
    let st_ = st.clone();
    let s0 = Signal::new(move |sc| st_.get(sc));
    let s1 = Signal::new(move |sc| s0.get(sc));
    let s2 = Signal::new(move |sc| s1.get(sc));
    let s3 = Signal::new(move |sc| s2.get(sc));

    assert_eq!(s3.get(&mut rt.sc()), 5);

    st.set(10, rt.ac());
    assert_eq!(s3.get(&mut rt.sc()), 10);
}

#[test]
fn new_borrow2() {
    let mut rt = Runtime::new();

    let s = Signal::new(move |_| 10);

    let mut sc = rt.sc();
    let b0 = s.borrow(&mut sc);
    let b1 = s.borrow(&mut sc);
    drop(b0);
    drop(b1);
}

#[test]
fn new_effect() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let a = State::new(5);
    let b = State::new(10);
    let s = Signal::new({
        let a = a.clone();
        let b = b.clone();
        move |sc| a.get(sc) + b.get(sc)
    });
    let _e = s.effect(|x| call!("{x}"));

    rt.flush();
    cr.verify("15");

    a.set(10, rt.ac());
    rt.flush();
    cr.verify("20");

    b.set(20, rt.ac());
    rt.flush();
    cr.verify("30");

    a.set(15, rt.ac());
    b.set(25, rt.ac());
    rt.flush();
    cr.verify("40");
}

#[test]
fn new_discard() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let s = Signal::new(move |_| call_on_drop("drop"));
    s.borrow(&mut rt.sc());
    cr.verify(());
    rt.flush();
    cr.verify("drop");
}

#[test]
fn on_discard_on_no_dependants() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let s = SignalBuilder::new(move |_| ())
        .on_discard(|_| call!("discard"))
        .build();
    s.get(&mut rt.sc());
    cr.verify(());
    rt.flush();
    cr.verify("discard");
}
#[test]
fn on_discard_on_drop() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let s = SignalBuilder::new(move |_| ())
        .on_discard(|_| call!("discard"))
        .build();
    s.get(&mut rt.sc());
    cr.verify(());
    drop(s);
    cr.verify(());
    rt.flush();
    cr.verify("discard");
}

#[test]
fn keep() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let s = SignalBuilder::new(|_| ())
        .on_discard(|_| call!("discard"))
        .build()
        .keep();
    s.borrow(&mut rt.sc());
    cr.verify(());
    rt.flush();
    cr.verify(());
    drop(s);
    rt.flush();
    cr.verify("discard");
}

#[test]
fn builder_keep() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let s = SignalBuilder::new(move |_| call_on_drop("drop"))
        .keep()
        .build();
    s.borrow(&mut rt.sc());
    cr.verify(());
    rt.flush();
    cr.verify(());
    drop(s);
    cr.verify("drop");
}

#[test]
fn new_no_dedup() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let st = State::new(5);

    let s = Signal::new({
        let st = st.clone();
        move |sc| st.get(sc)
    });
    let _e = s.effect(|x| call!("{x}"));
    rt.flush();
    cr.verify("5");

    st.set(5, rt.ac());
    rt.flush();
    cr.verify("5");

    st.set(10, rt.ac());
    rt.flush();
    cr.verify("10");
}

#[test]
fn new_dedup() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let st = State::new(5);

    let s = Signal::new_dedup({
        let st = st.clone();
        move |sc| st.get(sc)
    });
    let _e = s.effect(|x| call!("{x}"));
    rt.flush();
    cr.verify("5");

    st.set(5, rt.ac());
    rt.flush();
    cr.verify(());

    st.set(10, rt.ac());
    rt.flush();
    cr.verify("10");
}

#[test]
fn from_value() {
    let mut rt = Runtime::new();

    let s = Signal::from_value(5);
    assert_eq!(s.get(&mut rt.sc()), 5);
}

#[test]
fn from_value_map() {
    let mut rt = Runtime::new();

    let value = (5, 10);
    let s = Signal::from_value_map(value, |x| &x.0);

    assert_eq!(s.get(&mut rt.sc()), 5);
}

#[test]
fn from_owned() {
    let mut rt = Runtime::new();

    let owned = String::from("hello");
    let s = Signal::<str>::from_owned(owned);

    assert_eq!(s.get(&mut rt.sc()), "hello");
}

#[test]
fn from_borrow() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let st = State::new(5);
    let s = Signal::from_borrow(st.clone(), |st, sc, _| st.borrow(sc));

    let _e = s.effect(|x| call!("{x}"));
    rt.flush();
    cr.verify("5");

    st.set(10, rt.ac());
    rt.flush();
    cr.verify("10");
}

#[test]
fn from_static_ref() {
    let mut rt = Runtime::new();
    let s = Signal::from_static_ref(&5);
    assert_eq!(s.get(&mut rt.sc()), 5);
}

#[test]
async fn from_future() {
    let mut rt = Runtime::new();

    let (sender, receiver) = oneshot_broadcast::<i32>();

    let s = Signal::from_future(async move { receiver.recv().await });

    assert_eq!(s.get(&mut rt.sc()), Poll::Pending);
    rt.flush();
    assert_eq!(s.get(&mut rt.sc()), Poll::Pending);
    sender.send(20);
    rt.flush();
    assert_eq!(s.get(&mut rt.sc()), Poll::Ready(20));
}

#[test]
async fn from_future_borrow2() {
    let mut rt = Runtime::new();

    let (_sender, receiver) = oneshot_broadcast::<i32>();

    let s = Signal::from_future(async move { receiver.recv().await });

    let mut sc = rt.sc();
    let b0 = s.borrow(&mut sc);
    let b1 = s.borrow(&mut sc);

    assert_eq!(*b0, Poll::Pending);
    assert_eq!(*b1, Poll::Pending);
}

#[test]
async fn from_stream() {
    let mut rt = Runtime::new();
    let s0 = State::new(10);
    let s1 = Signal::from_stream(s0.to_signal().to_stream());

    assert_eq!(s1.get(&mut rt.sc()), Poll::<i32>::Pending);

    s0.set(20, rt.ac());
    wait_for_idle().await;
    rt.flush();
    wait_for_idle().await;
    rt.flush();
    assert_eq!(s1.get(&mut rt.sc()), Poll::<i32>::Ready(20));

    s0.set(20, rt.ac());
    wait_for_idle().await;
    rt.flush();
    wait_for_idle().await;
    rt.flush();
    assert_eq!(s1.get(&mut rt.sc()), Poll::<i32>::Ready(20));
}

#[test]
async fn from_stream_borrow2() {
    let mut rt = Runtime::new();
    let s = Signal::from_stream(State::new(10).to_signal().to_stream());

    let sc = &mut rt.sc();
    let _b0 = s.borrow(sc);
    let _b1 = s.borrow(sc);
}

#[test]
async fn from_async() {
    let mut rt = Runtime::new();

    let (sender, receiver) = oneshot_broadcast::<i32>();

    let s = Signal::from_async(async move |_| receiver.recv().await);

    assert_eq!(s.get(&mut rt.sc()), Poll::Pending);
    rt.flush();
    assert_eq!(s.get(&mut rt.sc()), Poll::Pending);
    sender.send(20);
    rt.flush();
    assert_eq!(s.get(&mut rt.sc()), Poll::Ready(20));
}

#[test]
fn from_async_effect() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let (sender, receiver) = oneshot_broadcast::<i32>();

    let s = Signal::from_async(async move |_| receiver.recv().await);

    let _e = effect({
        let s = s.clone();
        move |sc| {
            call!("{:?}", s.get(sc));
        }
    });

    rt.flush();
    cr.verify(format!("{:?}", Poll::<i32>::Pending));

    sender.send(20);
    rt.flush();
    cr.verify(format!("{:?}", Poll::<i32>::Ready(20)));
}

#[test]
fn from_async_no_dependants() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let (_sender, receiver) = oneshot_broadcast::<i32>();

    let s = Signal::from_async(async move |_| {
        let _x = call_on_drop("drop");
        receiver.recv().await
    });

    assert_eq!(s.get(&mut rt.sc()), Poll::Pending);
    cr.verify(());
    rt.flush();
    cr.verify("drop");
}

#[test]
fn get_async() {
    let mut rt = Runtime::new();

    let s0 = State::new(Poll::<i32>::Pending);

    let s = Signal::from_async({
        let s0 = s0.clone();
        async move |sc| s0.to_signal().get_async(sc).await
    });

    assert_eq!(s.get(&mut rt.sc()), Poll::Pending);

    s0.set(Poll::Ready(20), rt.ac());
    assert_eq!(s.get(&mut rt.sc()), Poll::Ready(20));

    s0.set(Poll::Ready(30), rt.ac());
    assert_eq!(s.get(&mut rt.sc()), Poll::Ready(30));

    s0.set(Poll::Pending, rt.ac());
    assert_eq!(s.get(&mut rt.sc()), Poll::Pending);
}

#[test]
async fn to_stream() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let s = State::new(5);
    let _reaction = spawn_local(
        s.to_signal()
            .to_stream()
            .for_each(|x| async move { call!("{x}") }),
    );
    wait_for_idle().await;

    rt.flush();
    wait_for_idle().await;

    cr.verify("5");

    s.set(10, rt.ac());
    rt.flush();
    wait_for_idle().await;
    cr.verify("10");
}

#[test]
#[should_panic]
fn cyclic() {
    let mut rt = Runtime::new();

    let s0 = Rc::new(RefCell::new(Signal::from_value(0)));
    let s = Signal::new({
        let s0 = s0.clone();
        move |sc| s0.borrow().get(sc)
    });
    s0.borrow_mut().clone_from(&s);

    s.get(&mut rt.sc());
}

#[test]
fn debug_signal() {
    let s = Signal::from_value(42);
    let debug_str = format!("{s:?}");
    assert!(debug_str.contains("42"));
}

#[test]
fn signal_ptr_eq() {
    let s1 = Signal::from_value(10);
    let s2 = s1.clone();
    let s3 = Signal::from_value(10);

    assert!(Signal::ptr_eq(&s1, &s2));
    assert!(!Signal::ptr_eq(&s1, &s3));
}

#[test]
fn ptr_eq_static_ref() {
    static VALUE_A: i32 = 42;
    static VALUE_B: i32 = 42;
    let s1 = Signal::from_static_ref(&VALUE_A);
    let s2 = Signal::from_static_ref(&VALUE_A);
    let s3 = Signal::from_static_ref(&VALUE_B);
    let s4 = Signal::from_value(42);

    assert!(Signal::ptr_eq(&s1, &s2));
    assert!(!Signal::ptr_eq(&s1, &s3));
    assert!(!Signal::ptr_eq(&s1, &s4));
}

#[test]
fn dedup_method() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let st = State::new(5);
    let signal = st.to_signal().dedup();

    let _sub = signal.effect(|x| call!("{x}"));

    rt.flush();
    cr.verify("5");

    st.set(5, rt.ac());
    rt.flush();
    cr.verify(());

    st.set(10, rt.ac());
    rt.flush();
    cr.verify("10");
}

#[test]
fn effect_in_custom_phase() {
    const CUSTOM_PHASE: ReactionPhase = ReactionPhase::new(1, "custom");
    let mut rt = Runtime::new();
    Runtime::register_reaction_phase(CUSTOM_PHASE);
    let mut cr = CallRecorder::new();

    let st = State::new(0);
    let signal = st.to_signal();

    let _sub = signal.effect_in(|x| call!("{x}"), CUSTOM_PHASE);

    rt.dispatch_reactions(ReactionPhase::default());
    cr.verify(());

    rt.dispatch_reactions(CUSTOM_PHASE);
    cr.verify("0");

    st.set(10, rt.ac());
    rt.dispatch_reactions(ReactionPhase::default());
    cr.verify(());

    rt.dispatch_reactions(CUSTOM_PHASE);
    cr.verify("10");
}

#[test]
async fn to_stream_map() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let s = State::new(5);
    let _reaction = spawn_local(
        s.to_signal()
            .to_stream_map(|x| x * 2)
            .for_each(|x| async move { call!("{x}") }),
    );
    wait_for_idle().await;

    rt.flush();
    wait_for_idle().await;

    cr.verify("10");

    s.set(10, rt.ac());
    rt.flush();
    wait_for_idle().await;
    cr.verify("20");
}

#[test]
fn debug_static_ref_signal() {
    static VALUE: i32 = 42;
    let signal = Signal::from_static_ref(&VALUE);
    let debug_str = format!("{signal:?}");
    assert_eq!(debug_str, "42");
}

#[test]
fn debug_from_borrow_signal() {
    let signal = Signal::from_borrow(42i32, |x, _, _| StateRef::from(x));
    let debug_str = format!("{signal:?}");
    assert!(debug_str.contains("borrow"));
}

#[test]
async fn from_future_scan_filter_reject() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let (sender, receiver) = oneshot_broadcast::<i32>();

    let signal = SignalBuilder::from_future_scan_filter(
        0,
        async move { receiver.recv().await },
        |st, value| {
            if value > 10 {
                *st = value;
                true
            } else {
                false
            }
        },
    )
    .build();

    let _sub = signal.effect(|x| call!("{x}"));

    rt.flush();
    cr.verify("0");

    sender.send(5);
    rt.flush();
    cr.verify(());
}

#[test]
async fn from_future_scan_filter_accept() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let (sender, receiver) = oneshot_broadcast::<i32>();
    let signal = SignalBuilder::from_future_scan_filter(
        0,
        async move { receiver.recv().await },
        |st, value| {
            *st = value;
            true
        },
    )
    .build();

    let _sub = signal.effect(|x| call!("{x}"));
    rt.flush();
    cr.verify("0");

    sender.send(42);
    rt.flush();
    cr.verify("42");
}

#[test]
fn on_discard_value() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let signal = SignalBuilder::new(|_| 42)
        .dedup()
        .on_discard_value(|v| call!("discard:{v}"))
        .build();

    signal.get(&mut rt.sc());
    cr.verify(());

    rt.flush();
    cr.verify("discard:42");
}

#[test]
fn builder_map_value() {
    let mut rt = Runtime::new();

    let signal = SignalBuilder::new(|_| 21).map_value(|x| x * 2).build();

    assert_eq!(signal.get(&mut rt.sc()), 42);
}

#[test]
fn builder_flat_map() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let inner = State::new(42);
    let inner_signal = inner.to_signal();

    let outer = SignalBuilder::new(move |_| inner_signal.clone())
        .flat_map(|s| s)
        .build();

    let _sub = outer.effect(|x| call!("{x}"));
    rt.flush();
    cr.verify("42");

    inner.set(100, rt.ac());
    rt.flush();
    cr.verify("100");
}

#[test]
async fn future_scan_with_map() {
    let mut rt = Runtime::new();

    let (sender, receiver) = oneshot_broadcast::<i32>();
    let signal =
        SignalBuilder::from_future_scan(0, async move { receiver.recv().await }, |st, v| *st = v)
            .map_value(|x| x * 2)
            .build();

    assert_eq!(signal.get(&mut rt.sc()), 0);

    sender.send(21);
    rt.flush();
    assert_eq!(signal.get(&mut rt.sc()), 42);
}

#[test]
fn debug_future_scan_signal() {
    let signal = SignalBuilder::from_future_scan(0, async { 42 }, |st, v| *st = v).build();
    let debug_str = format!("{signal:?}");
    assert!(debug_str.contains("future_scan"));
}

#[test]
fn debug_stream_scan_signal() {
    let signal =
        SignalBuilder::from_stream_scan_filter(0, futures::stream::once(async { 42 }), |st, v| {
            *st = v.unwrap_or(0);
            true
        })
        .build();
    let debug_str = format!("{signal:?}");
    assert!(debug_str.contains("stream_scan"));
}
