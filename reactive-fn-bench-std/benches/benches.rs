#![feature(test)]
extern crate test;
use test::Bencher;

use reactive_fn::{core::Runtime, *};
use std::hint::black_box;

const COUNT: usize = 1000;
const SUBSCRIPTIONS: usize = 1000;
const SOURCES: usize = 1000;

// const COUNT: usize = 1000;
// const SUBSCRIPTIONS: usize = 10000;
// const SOURCES: usize = 10000;

fn iter_dc(b: &mut Bencher, f: impl Fn(&mut Runtime)) {
    b.iter(|| Runtime::with(&f));
}

#[bench]
fn simple_impl(b: &mut Bencher) {
    let cell = ObsCell::new(0);
    iter_dc(b, |dc| {
        let _s = cell.obs_builder().map(|x| x + 1).subscribe(|x| {
            black_box(x);
        });
        for i in 0..COUNT {
            cell.set(i, &mut dc.ac());
            dc.update();
        }
    });
}

#[bench]
fn simple_dyn(b: &mut Bencher) {
    let cell = ObsCell::new(0);
    iter_dc(b, |dc| {
        let _s = cell.obs().map(|x| x + 1).subscribe(|x| {
            black_box(x);
        });
        for i in 0..COUNT {
            cell.set(i, &mut dc.ac());
            dc.update();
        }
    });
}

#[bench]
fn many_subscription_impl(b: &mut Bencher) {
    let cell = ObsCell::new(0);
    iter_dc(b, |dc| {
        let mut ss = Vec::new();
        for _ in 0..SUBSCRIPTIONS {
            ss.push(cell.obs_builder().map(|x| x + 1).subscribe(|x| {
                black_box(x);
            }));
        }
        for i in 0..COUNT {
            cell.set(i, &mut dc.ac());
            dc.update();
        }
    });
}

#[bench]
fn many_subscription_dyn(b: &mut Bencher) {
    let cell = ObsCell::new(0);
    iter_dc(b, |dc| {
        let mut ss = Vec::new();
        for _ in 0..SUBSCRIPTIONS {
            ss.push(cell.obs().map(|x| x + 1).subscribe(|x| {
                black_box(x);
            }));
        }
        for i in 0..COUNT {
            cell.set(i, &mut dc.ac());
            dc.update();
        }
    });
}

#[bench]
fn many_source_impl(b: &mut Bencher) {
    let mut cells = Vec::new();
    for i in 0..SOURCES {
        cells.push(ObsCell::new(i));
    }
    iter_dc(b, |dc| {
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
            cells[i % cells.len()].set(i, &mut dc.ac());
            dc.update();
        }
    });
}

#[bench]
fn many_source_dyn(b: &mut Bencher) {
    let mut cells = Vec::new();
    for i in 0..SOURCES {
        cells.push(ObsCell::new(i));
    }
    iter_dc(b, |dc| {
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
            cells[i % cells.len()].set(i, &mut dc.ac());
            dc.update();
        }
    });
}

const DEPTH: usize = 100;

#[bench]
fn many_depth(b: &mut Bencher) {
    let cell = ObsCell::new(0);
    iter_dc(b, |dc| {
        let mut s = cell.obs();
        for _ in 0..DEPTH {
            s = s.map(|x| x + 1).memo();
        }
        let _s = s.subscribe(|x| {
            black_box(x);
        });
        for i in 0..COUNT {
            cell.set(i, &mut dc.ac());
            dc.update();
        }
    });
}
