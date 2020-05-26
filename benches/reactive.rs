#![feature(test)]

extern crate test;
use reactive_fn::*;
use test::Bencher;

fn re_cell_fold_n(n: usize) {
    let cell = ReCell::new(0);
    let fold = cell.to_re().fold(0, |s, x| s + x);

    for i in 1..n {
        cell.set_and_update(i);
    }
    fold.stop();
}

#[bench]
fn re_cell_fold_1000(b: &mut Bencher) {
    b.iter(|| re_cell_fold_n(1000));
}

fn re_cell_flatten_n(n: usize) -> usize {
    let s = ReRefCell::new(Re::constant(0));
    let s1 = Re::constant(1);
    let s2 = Re::constant(2);
    let f = s.to_re_borrow().flatten().fold(0, |s, x| s + x);

    for _ in 0..n {
        s.set_and_update(s1.clone());
        s.set_and_update(s2.clone());
    }
    f.stop()
}

#[bench]
fn re_cell_flatten_1000(b: &mut Bencher) {
    b.iter(|| re_cell_flatten_n(1000));
}
