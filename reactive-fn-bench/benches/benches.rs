// #![feature(test)]
// extern crate test;
// use test::Bencher;

use criterion::{criterion_group, criterion_main, Bencher, Criterion};
use reactive_fn::{core::Runtime, *};
use std::hint::black_box;

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("simple_impl", simple_impl);
    c.bench_function("simple_dyn", simple_dyn);
    c.bench_function("many_subscription_impl", many_subscription_impl);
    c.bench_function("many_subscription_dyn", many_subscription_dyn);
    c.bench_function("many_source_impl", many_source_impl);
    c.bench_function("many_source_dyn", many_source_dyn);
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);

// const COUNT: usize = 1000;
// const SUBSCRIPTIONS: usize = 10000;
// const SOURCES: usize = 10000;

const COUNT: usize = 1000;
const SUBSCRIPTIONS: usize = 10000;
const SOURCES: usize = 10000;

fn iter_dc(b: &mut Bencher, f: impl Fn(&mut Runtime)) {
    b.iter(|| Runtime::with(&f));
}

// #[bench]
fn simple_impl(b: &mut Bencher) {
    let cell = ObsCell::new(0);
    iter_dc(b, |rt| {
        let _s = cell.obs_builder().map(|x| x + 1).subscribe(|x| {
            black_box(x);
        });
        for i in 0..COUNT {
            cell.set(i, &mut rt.ac());
            rt.update();
        }
    });
}

// #[bench]
fn simple_dyn(b: &mut Bencher) {
    let cell = ObsCell::new(0);
    iter_dc(b, |rt| {
        let _s = cell.obs().map(|x| x + 1).subscribe(|x| {
            black_box(x);
        });
        for i in 0..COUNT {
            cell.set(i, &mut rt.ac());
            rt.update();
        }
    });
}

// #[bench]
fn many_subscription_impl(b: &mut Bencher) {
    let cell = ObsCell::new(0);
    iter_dc(b, |rt| {
        let mut ss = Vec::new();
        for _ in 0..SUBSCRIPTIONS {
            ss.push(cell.obs_builder().map(|x| x + 1).subscribe(|x| {
                black_box(x);
            }));
        }
        for i in 0..COUNT {
            cell.set(i, &mut rt.ac());
            rt.update();
        }
    });
}

// #[bench]
fn many_subscription_dyn(b: &mut Bencher) {
    let cell = ObsCell::new(0);
    iter_dc(b, |rt| {
        let mut ss = Vec::new();
        for _ in 0..SUBSCRIPTIONS {
            ss.push(cell.obs().map(|x| x + 1).subscribe(|x| {
                black_box(x);
            }));
        }
        for i in 0..COUNT {
            cell.set(i, &mut rt.ac());
            rt.update();
        }
    });
}

// #[bench]
fn many_source_impl(b: &mut Bencher) {
    let mut cells = Vec::new();
    for i in 0..SOURCES {
        cells.push(ObsCell::new(i));
    }
    iter_dc(b, |rt| {
        let cells_1 = cells.clone();
        let sum = ObsBuilder::from_get(move |bc| {
            let mut sum = 0;
            for cell in &cells_1 {
                sum += cell.get(bc);
            }
            sum
        });
        let _s = sum.subscribe(|x| {
            black_box(x);
        });
        for i in 0..COUNT {
            cells[i % cells.len()].set(i, &mut rt.ac());
            rt.update();
        }
    });
}

// #[bench]
fn many_source_dyn(b: &mut Bencher) {
    let mut cells = Vec::new();
    for i in 0..SOURCES {
        cells.push(ObsCell::new(i));
    }
    iter_dc(b, |rt| {
        let cells_1 = cells.clone();
        let sum = Obs::from_get(move |bc| {
            let mut sum = 0;
            for cell in &cells_1 {
                sum += cell.get(bc);
            }
            sum
        });
        let _s = sum.subscribe(|x| {
            black_box(x);
        });
        for i in 0..COUNT {
            cells[i % cells.len()].set(i, &mut rt.ac());
            rt.update();
        }
    });
}
