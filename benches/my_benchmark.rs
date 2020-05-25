use criterion::{criterion_group, criterion_main, Criterion};
use reactive_fn::*;

fn re_cell_fold(n: usize) {
    let cell = ReCell::new(0);
    let fold = cell.to_re().fold(0, |s, x| s + x);

    for i in 1..n {
        cell.set_and_update(i);
    }
    assert_eq!(fold.stop(), (0 + n - 1) * n / 2);
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("re_cell_fold 100", |b| b.iter(|| re_cell_fold(100)));
    c.bench_function("re_cell_fold 500", |b| b.iter(|| re_cell_fold(500)));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
