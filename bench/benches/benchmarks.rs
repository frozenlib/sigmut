#![cfg(feature = "benchmark")]
#![feature(test)]

use std::hint::black_box;

extern crate test;

#[bench]
fn one_to_one(b: &mut test::Bencher) {
    let mut rt = sigmut::core::Runtime::new();
    let mut states = Vec::new();
    let mut subscriptions = Vec::new();
    for i in 0..100 {
        let s = sigmut::State::new(i);
        let sub = s.to_signal().effect(|v| {
            black_box(v);
        });
        states.push(s);
        subscriptions.push(sub);
    }
    b.iter(|| {
        for n in 0..10 {
            let ac = rt.ac();
            for s in &states {
                s.set(n, ac);
            }
            rt.update();
        }
    });
}
