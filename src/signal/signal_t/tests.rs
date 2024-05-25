use std::{
    cell::RefCell,
    future::poll_fn,
    rc::Rc,
    task::{Poll, Waker},
};

use crate::{core::Runtime, Signal, State};
use derive_ex::{derive_ex, Ex};
use rt_local::runtime::core::test;

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
fn new_dedup() {
    let mut rt = Runtime::new();

    let st = State::new(5);
    let st_ = st.clone();
    let s = Signal::new_dedup(move |sc| st_.get(sc));

    assert_eq!(s.get(&mut rt.sc()), 5);

    st.set(10, rt.ac());
    assert_eq!(s.get(&mut rt.sc()), 10);
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
    rt.wait_for_ready().await;
    rt.update();
    assert_eq!(s.get(&mut rt.sc()), Poll::Ready(20));
}
