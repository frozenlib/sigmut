#![feature(test)]

extern crate test;

use reactive_fn::*;
use test::black_box;

const COUNT: usize = 100000;

#[bench]
fn bench_obs(b: &mut test::Bencher) {
    let cell = ObsCell::new(0);
    b.iter(|| {
        let _s = cell.obs().map(|x| x + 1).subscribe(|x| {
            black_box(x);
        });
        for i in 0..COUNT {
            cell.set(i);
        }
    });
}

#[bench]
fn bench_obs_ref(b: &mut test::Bencher) {
    let cell = ObsCell::new(0);
    b.iter(|| {
        let _s = cell.obs().map(|x| x + 1).as_ref().subscribe(|x| {
            black_box(x);
        });
        for i in 0..COUNT {
            cell.set(i);
        }
    });
}

#[bench]
fn bench_dyn_obs(b: &mut test::Bencher) {
    let cell = ObsCell::new(0);
    b.iter(|| {
        let _s = cell.obs().map(|x| x + 1).into_dyn().subscribe(|x| {
            black_box(x);
        });
        for i in 0..COUNT {
            cell.set(i);
        }
    });
}

#[bench]
fn bench_dyn_obs_ref(b: &mut test::Bencher) {
    let cell = ObsCell::new(0);
    b.iter(|| {
        let _s = cell
            .obs()
            .map(|x| x + 1)
            .as_ref()
            .into_dyn()
            .subscribe(|x| {
                black_box(x);
            });
        for i in 0..COUNT {
            cell.set(i);
        }
    });
}
