use std::{
    any::Any,
    cell::RefCell,
    future::poll_fn,
    rc::Rc,
    task::{Poll, Waker},
};

use crate::{core::Runtime, effect, Signal, SignalBuilder, State};
use assert_call::{call, CallRecorder};
use derive_ex::{derive_ex, Ex};
use futures::StreamExt;
use rt_local::{runtime::core::test, spawn_local, wait_for_idle};

fn on_drop(s: &'static str) -> impl Any {
    struct OnDrop(&'static str);
    impl Drop for OnDrop {
        fn drop(&mut self) {
            call!("{}", self.0);
        }
    }
    OnDrop(s)
}

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
fn new_discard() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let s = Signal::new(move |_| on_drop("drop"));
    s.borrow(&mut rt.sc());
    cr.verify(());
    rt.update();
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
    rt.update();
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
    rt.update();
    cr.verify("discard");
}

#[test]
fn keep() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let s = SignalBuilder::new(move |_| on_drop("drop")).keep().build();
    s.borrow(&mut rt.sc());
    cr.verify(());
    rt.update();
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
    rt.update();
    cr.verify("5");

    st.set(5, rt.ac());
    rt.update();
    cr.verify("5");

    st.set(10, rt.ac());
    rt.update();
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
    rt.update();
    cr.verify("5");

    st.set(5, rt.ac());
    rt.update();
    cr.verify(());

    st.set(10, rt.ac());
    rt.update();
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
fn from_static_ref() {
    let mut rt = Runtime::new();
    let s = Signal::from_static_ref(&5);
    assert_eq!(s.get(&mut rt.sc()), 5);
}

struct OneshotBroadcast<T> {
    value: Option<T>,
    waker: Option<Waker>,
}

fn oneshot_broadcast<T>() -> (Sender<T>, Receiver<T>) {
    let data = Rc::new(RefCell::new(OneshotBroadcast {
        value: None,
        waker: None,
    }));
    (Sender(data.clone()), Receiver(data))
}

struct Sender<T>(Rc<RefCell<OneshotBroadcast<T>>>);

impl<T> Sender<T> {
    fn send(&self, value: T) {
        let mut data = self.0.borrow_mut();
        data.value = Some(value);
        if let Some(waker) = data.waker.take() {
            waker.wake();
        }
    }
}

#[derive(Ex)]
#[derive_ex(Clone(bound()))]
struct Receiver<T>(Rc<RefCell<OneshotBroadcast<T>>>);

impl<T: Clone> Receiver<T> {
    async fn recv(&self) -> T {
        poll_fn(|cx| {
            let mut d = self.0.borrow_mut();
            if let Some(value) = &d.value {
                Poll::Ready(value.clone())
            } else {
                d.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        })
        .await
    }
}

#[test]
async fn from_async() {
    let mut rt = Runtime::new();

    let (sender, receiver) = oneshot_broadcast::<i32>();

    let s = Signal::from_async(move |_| {
        let receiver = receiver.clone();
        async move { receiver.recv().await }
    });

    assert_eq!(s.get(&mut rt.sc()), Poll::Pending);
    rt.update();
    assert_eq!(s.get(&mut rt.sc()), Poll::Pending);
    sender.send(20);
    rt.update();
    assert_eq!(s.get(&mut rt.sc()), Poll::Ready(20));
}

#[test]
fn from_async_effect() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let (sender, receiver) = oneshot_broadcast::<i32>();

    let s = Signal::from_async(move |_| {
        let receiver = receiver.clone();
        async move { receiver.recv().await }
    });

    let _e = effect({
        let s = s.clone();
        move |sc| {
            call!("{:?}", s.get(sc));
        }
    });

    rt.update();
    cr.verify(format!("{:?}", Poll::<i32>::Pending));

    sender.send(20);
    rt.update();
    cr.verify(format!("{:?}", Poll::<i32>::Ready(20)));
}

#[test]
fn from_async_no_dependants() {
    let mut rt = Runtime::new();
    let mut cr = CallRecorder::new();

    let (_sender, receiver) = oneshot_broadcast::<i32>();

    let s = Signal::from_async(move |_| {
        let receiver = receiver.clone();
        async move {
            let _x = on_drop("drop");
            receiver.recv().await
        }
    });

    assert_eq!(s.get(&mut rt.sc()), Poll::Pending);
    cr.verify(());
    rt.update();
    cr.verify("drop");
}

#[test]
async fn from_future() {
    let mut rt = Runtime::new();

    let (sender, receiver) = oneshot_broadcast::<i32>();

    let s = Signal::from_future(async move { receiver.recv().await });

    assert_eq!(s.get(&mut rt.sc()), Poll::Pending);
    rt.update();
    assert_eq!(s.get(&mut rt.sc()), Poll::Pending);
    sender.send(20);
    rt.update();
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
fn get_async() {
    let mut rt = Runtime::new();

    let s0 = State::new(Poll::<i32>::Pending);

    let s = Signal::from_async({
        let s0 = s0.clone();
        move |mut sc| {
            let s0 = s0.clone();
            async move { s0.to_signal().get_async(&mut sc).await }
        }
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
    let _task = spawn_local(
        s.to_signal()
            .to_stream()
            .for_each(|x| async move { call!("{x}") }),
    );
    wait_for_idle().await;

    rt.update();
    wait_for_idle().await;

    cr.verify("5");

    s.set(10, rt.ac());
    rt.update();
    wait_for_idle().await;
    cr.verify("10");
}

#[test]
async fn from_stream() {
    let mut rt = Runtime::new();
    let s0 = State::new(10);
    let s1 = Signal::from_stream(s0.to_signal().to_stream());

    assert_eq!(s1.get(&mut rt.sc()), Poll::<i32>::Pending);

    s0.set(20, rt.ac());
    wait_for_idle().await;
    rt.update();
    wait_for_idle().await;
    rt.update();
    assert_eq!(s1.get(&mut rt.sc()), Poll::<i32>::Ready(20));

    s0.set(20, rt.ac());
    wait_for_idle().await;
    rt.update();
    wait_for_idle().await;
    rt.update();
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
