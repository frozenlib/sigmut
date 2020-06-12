use criterion::{black_box, criterion_group, criterion_main, Criterion};
use reactive_fn::*;

criterion_main!(benches);
criterion_group!(benches, criterion_benchmark);

pub fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("dyn_re_fold 100000", |b| b.iter(|| dyn_re_fold_n(100000)));
    c.bench_function("ops_re_fold 100000", |b| b.iter(|| ops_re_fold_n(100000)));
    c.bench_function("dyn_re_map_fold 100000", |b| {
        b.iter(|| dyn_re_map_fold_n(100000))
    });
    c.bench_function("ops_re_map_fold 100000", |b| {
        b.iter(|| ops_re_map_fold_n(100000))
    });
}

fn dyn_re_fold_n(n: usize) {
    let cell = ReCell::new(0);
    let fold = cell.to_re().fold(0, |s, x| s + x);

    for i in 1..n {
        cell.set_and_update(i);
    }
    black_box(fold.stop());
}
fn ops_re_fold_n(n: usize) {
    let cell = ReCell::new(0);
    let fold = cell.ops().fold(0, |s, x| s + x);

    for i in 1..n {
        cell.set_and_update(i);
    }
    black_box(fold.stop());
}

fn dyn_re_map_fold_n(n: usize) {
    let cell = ReCell::new(0);
    let fold = cell.to_re().map(|x| x * 2).fold(0, |s, x| s + x);

    for i in 1..n {
        cell.set_and_update(i);
    }
    black_box(fold.stop());
}
fn ops_re_map_fold_n(n: usize) {
    let cell = ReCell::new(0);
    let fold = cell.ops().map(|x| x * 2).fold(0, |s, x| s + x);

    for i in 1..n {
        cell.set_and_update(i);
    }
    black_box(fold.stop());
}
