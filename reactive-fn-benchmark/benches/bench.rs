#![feature(test)]

extern crate test;

use reactive_fn::*;
use reactive_fn::exports::rt_local_core::wait_for_idle;
use std::future::Future;
use test::black_box;

const COUNT: usize = 1000;
const SUBSCRIPTIONS: usize = 10000;
const SOURCES: usize = 10000;

fn iter_async<Fut: Future>(b: &mut test::Bencher, mut f: impl FnMut() -> Fut) {
    b.iter(|| rt_local::runtime::core::run(f()));
}

#[bench]
fn simple_impl(b: &mut test::Bencher) {
    let cell = ObsCell::new(0);
    iter_async(b, || async {
        let _s = cell.obs().map(|x| x + 1).subscribe(|x| {
            black_box(x);
        });
        for i in 0..COUNT {
            cell.set(i);
            wait_for_idle().await;
        }
    });
}

#[bench]
fn simple_dyn(b: &mut test::Bencher) {
    let cell = ObsCell::new(0);
    iter_async(b, || async {
        let _s = cell.obs().map(|x| x + 1).into_dyn().subscribe(|x| {
            black_box(x);
        });
        for i in 0..COUNT {
            cell.set(i);
            wait_for_idle().await;
        }
    });
}

#[bench]
fn many_subscription_impl(b: &mut test::Bencher) {
    let cell = ObsCell::new(0);
    iter_async(b, || async {
        let mut ss = Vec::new();
        for _ in 0..SUBSCRIPTIONS {
            ss.push(cell.obs().map(|x| x + 1).subscribe(|x| {
                black_box(x);
            }));
        }
        for i in 0..COUNT {
            cell.set(i);
            wait_for_idle().await;
        }
    });
}
#[bench]
fn many_subscription_dyn(b: &mut test::Bencher) {
    let cell = ObsCell::new(0);
    iter_async(b, || async {
        let mut ss = Vec::new();
        for _ in 0..SUBSCRIPTIONS {
            ss.push(cell.obs().map(|x| x + 1).into_dyn().subscribe(|x| {
                black_box(x);
            }));
        }
        for i in 0..COUNT {
            cell.set(i);
            wait_for_idle().await;
        }
    });
}

#[bench]
fn many_source_impl(b: &mut test::Bencher) {
    let mut cells = Vec::new();
    for i in 0..SOURCES {
        cells.push(ObsCell::new(i));
    }
    iter_async(b, || async {
        let cells_1 = cells.clone();
        let sum = obs(move |bc| {
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
            cells[i % cells.len()].set(i);
            wait_for_idle().await;
        }
    });
}

#[bench]
fn many_source_dyn(b: &mut test::Bencher) {
    let mut cells = Vec::new();
    for i in 0..SOURCES {
        cells.push(ObsCell::new(i));
    }
    iter_async(b, || async {
        let cells_1 = cells.clone();
        let sum = obs(move |bc| {
            let mut sum = 0;
            for cell in &cells_1 {
                sum += cell.get(bc);
            }
            sum
        });
        let _s = sum.into_dyn().subscribe(|x| {
            black_box(x);
        });
        for i in 0..COUNT {
            cells[i % cells.len()].set(i);
            wait_for_idle().await;
        }
    });
}
