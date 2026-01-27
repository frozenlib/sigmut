#![feature(test)]

use std::{hint::black_box, rc::Rc};

use sigmut::{Signal, State, effect};

extern crate test;

#[bench]
fn one_to_one(b: &mut test::Bencher) {
    let mut rt = sigmut::core::Runtime::new();
    let mut ss = Vec::new();
    let mut subs = Vec::new();
    for i in 0..10000 {
        let s = sigmut::State::new(i);
        let sub = s.to_signal().effect(|v| {
            black_box(v);
        });
        ss.push(s);
        subs.push(sub);
    }
    b.iter(|| {
        for n in 0..5 {
            let ac = rt.ac();
            for s in &ss {
                s.set(n, ac);
            }
            rt.flush();
        }
    });
}

#[bench]
fn one_to_many(b: &mut test::Bencher) {
    let mut rt = sigmut::core::Runtime::new();
    let mut subs = Vec::new();
    let s = State::new(0);
    for j in 0..10000 {
        let sub = s.to_signal().effect(move |v| {
            black_box((j, v));
        });
        subs.push(sub);
    }

    b.iter(|| {
        for n in 0..5 {
            let ac = rt.ac();
            s.set(n, ac);
            rt.flush();
        }
    });
}

#[bench]
fn many_to_one(b: &mut test::Bencher) {
    let mut rt = sigmut::core::Runtime::new();
    let mut ss = Vec::new();
    for i in 0..10000 {
        ss.push(sigmut::State::new(i));
    }
    let ss = Rc::new(ss);
    let _sub = effect({
        let ss = ss.clone();
        move |sc| {
            let mut x = 0;
            for s in &*ss {
                x += s.get(sc);
            }
            black_box(x);
        }
    });
    b.iter(|| {
        for n in 0..5 {
            ss[n].set(n + 1, rt.ac());
            rt.flush();
        }
    });
}

#[bench]
fn many_to_many(b: &mut test::Bencher) {
    let mut rt = sigmut::core::Runtime::new();
    let mut ss = Vec::new();
    for i in 0..100 {
        ss.push(sigmut::State::new(i));
    }
    let ss = Rc::new(ss);
    let mut subs = Vec::new();
    for j in 0..100 {
        subs.push(effect({
            let ss = ss.clone();
            move |sc| {
                let mut x = 0;
                for s in &*ss {
                    x += s.get(sc);
                }
                black_box((j, x));
            }
        }));
    }
    b.iter(|| {
        for n in 0..5 {
            ss[n].set(n + 1, rt.ac());
            rt.flush();
        }
    });
}

#[bench]
fn deep(b: &mut test::Bencher) {
    let mut rt = sigmut::core::Runtime::new();
    let depth = 100;
    let mut ss = Vec::new();
    let mut sigs = Vec::new();
    for i in 0..100 {
        let state = sigmut::State::new(i);
        sigs.push(state.to_signal());
        ss.push(state);
    }

    for _ in 1..depth {
        for sig in &mut sigs {
            let s = sig.clone();
            *sig = Signal::new(move |sc| s.get(sc) + 1);
        }
    }
    let mut subs = Vec::new();
    for sig in &sigs {
        subs.push(sig.effect(|v| {
            black_box(v);
        }));
    }
    b.iter(|| {
        for n in 0..5 {
            for s in &ss {
                s.set(n, rt.ac());
            }
            rt.flush();
        }
    });
}
