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
    assert_eq!(fold.stop(), (0 + n - 1) * n / 2);
}

#[bench]
fn re_cell_fold_1000(b: &mut Bencher) {
    b.iter(|| re_cell_fold_n(1000));
}
