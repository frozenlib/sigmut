use crate::{core::Runtime, Signal, State};

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
