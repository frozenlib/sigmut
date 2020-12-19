#![type_length_limit = "4893383"]

use criterion::{criterion_group, criterion_main, Bencher, BenchmarkId, Criterion};
use reactive_fn::*;

criterion_main!(benches);
criterion_group!(benches, criterion_benchmark);

const UPDATE_COUNT: usize = 100;

fn update_counts() -> Vec<usize> {
    vec![1, 50, 100, 500, 1000]
}

pub fn criterion_benchmark(c: &mut Criterion) {
    bench(c, "re_fold", &re_fold_inputs(), re_fold_ops, re_fold_dyn);
    bench(
        c,
        "re_map_chain",
        &re_map_chain_inputs(),
        re_map_chain_ops,
        re_map_chain_dyn,
    );

    bench(
        c,
        "re_flatten",
        &re_flatten_inputs(),
        re_flatten_ops,
        re_flatten_dyn,
    );

    bench(
        c,
        "many_source",
        &many_source_inputs(),
        many_source_ops,
        many_source_dyn,
    );
    bench(
        c,
        "many_sink",
        &many_sink_inputs(),
        many_sink_ops,
        many_sink_dyn,
    );
    bench(
        c,
        "many_source_sink",
        &many_source_sink_inputs(),
        many_source_sink_ops,
        many_source_sink_dyn,
    );
}

fn bench(
    c: &mut Criterion,
    name: &str,
    inputs: &[usize],
    ops_func: impl Fn(&mut Bencher, usize),
    dyn_func: impl Fn(&mut Bencher, usize),
) {
    let mut g = c.benchmark_group(name);
    for input in inputs {
        g.bench_with_input(BenchmarkId::new("ops", input), input, |b, n| {
            ops_func(b, *n)
        });
        g.bench_with_input(BenchmarkId::new("dyn", input), input, |b, n| {
            dyn_func(b, *n)
        });
    }
}

fn re_fold_inputs() -> Vec<usize> {
    update_counts()
}
fn re_fold_ops(b: &mut Bencher, update_count: usize) {
    b.iter(|| {
        let cell = ObsCell::new(0);
        let fold = cell.obs().fold(0, |s, x| s + x);

        for i in 1..update_count {
            cell.set(i);
        }
        fold.stop()
    })
}
fn re_fold_dyn(b: &mut Bencher, update_count: usize) {
    b.iter(|| {
        let cell = ObsCell::new(0);
        let fold = cell.as_dyn().fold(0, |s, x| s + x);

        for i in 1..update_count {
            cell.set(i);
        }
        fold.stop()
    })
}

fn re_map_chain_inputs() -> Vec<usize> {
    vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
}
fn re_map_chain_ops(b: &mut Bencher, chain: usize) {
    struct Runner<S> {
        cell: ObsCell<usize>,
        ops: Obs<S>,
        n: usize,
    }
    fn new_runner(n: usize) -> Runner<impl Observable<Item = usize>> {
        let cell = ObsCell::new(0usize);
        let ops = cell.obs();
        Runner { cell, ops, n }
    }
    impl<S: Observable<Item = usize>> Runner<S> {
        fn try_run(self) -> Result<Runner<impl Observable<Item = usize>>, usize> {
            if self.n == 0 {
                self.cell.set(0);
                let fold = self.ops.fold(0, |s, x| s + x);
                for i in 0..UPDATE_COUNT {
                    self.cell.set(i);
                }
                Err(fold.stop())
            } else {
                Ok(Runner {
                    cell: self.cell,
                    ops: self.ops.map(x2),
                    n: self.n - 1,
                })
            }
        }
    }
    fn x2(x: usize) -> usize {
        x * 2
    }
    fn run(n: usize) -> Result<(), usize> {
        new_runner(n)
            .try_run()?
            .try_run()?
            .try_run()?
            .try_run()?
            .try_run()?
            .try_run()?
            .try_run()?
            .try_run()?
            .try_run()?
            .try_run()?
            .try_run()?;
        Ok(())
    }
    b.iter(|| run(chain).expect_err("unexpected input"))
}
fn re_map_chain_dyn(b: &mut Bencher, chain: usize) {
    let cell = ObsCell::new(0usize);
    let mut obs = cell.as_dyn();
    for _ in 0..chain {
        obs = obs.map(|x| x * 2);
    }
    b.iter(|| {
        cell.set(0);
        let fold = obs.fold(0, |s, x| s + x);
        for i in 0..UPDATE_COUNT {
            cell.set(i);
        }
        fold.stop()
    })
}

fn re_flatten_inputs() -> Vec<usize> {
    update_counts()
}
fn re_flatten_ops(b: &mut Bencher, update_count: usize) {
    b.iter(|| {
        let s = ObsRefCell::new(DynObs::constant(0));
        let s1 = DynObs::constant(1);
        let s2 = DynObs::constant(2);
        let f = s.obs().flatten().fold(0, |s, x| s + x);

        for _ in 0..update_count {
            s.set(s1.clone());
            s.set(s2.clone());
        }
        f.stop()
    })
}
fn re_flatten_dyn(b: &mut Bencher, update_count: usize) {
    b.iter(|| {
        let s = ObsRefCell::new(DynObs::constant(0));
        let s1 = DynObs::constant(1);
        let s2 = DynObs::constant(2);
        let f = s.as_dyn().flatten().fold(0, |s, x| s + x);

        for _ in 0..update_count {
            s.set(s1.clone());
            s.set(s2.clone());
        }
        f.stop()
    })
}

fn many_source_inputs() -> Vec<usize> {
    vec![1, 2, 4, 10, 20, 50, 100]
}

fn many_source_ops(b: &mut Bencher, source_count: usize) {
    b.iter(|| {
        let mut ss = Vec::new();
        for _ in 0..source_count {
            ss.push(ObsCell::new(0));
        }

        let f = {
            let ss = ss.clone();
            obs(move |cx| {
                let mut sum = 0;
                for s in &ss {
                    sum += s.get(cx)
                }
                sum
            })
            .fold(0, |s, x| s + x)
        };

        for i in 0..UPDATE_COUNT {
            ss[i % source_count].set(i);
        }
        f.stop()
    })
}
fn many_source_dyn(b: &mut Bencher, source_count: usize) {
    b.iter(|| {
        let mut ss = Vec::new();
        for _ in 0..source_count {
            ss.push(ObsCell::new(0));
        }

        let f = {
            let ss = ss.clone();
            DynObs::new(move |cx| {
                let mut sum = 0;
                for s in &ss {
                    sum += s.get(cx)
                }
                sum
            })
            .fold(0, |s, x| s + x)
        };

        for i in 0..UPDATE_COUNT {
            ss[i % source_count].set(i);
        }
        f.stop()
    })
}

fn many_sink_inputs() -> Vec<usize> {
    vec![1, 2, 4, 10, 20, 50, 100]
}
fn many_sink_ops(b: &mut Bencher, sink_count: usize) {
    b.iter(|| {
        let s = ObsCell::new(0);
        let mut fs = Vec::new();

        for _ in 0..sink_count {
            fs.push(s.obs().fold(0, move |s, x| s + x));
        }
        for i in 0..UPDATE_COUNT {
            s.set(i);
        }

        let mut sum = 0;
        for f in fs {
            sum += f.stop();
        }
        sum
    });
}
fn many_sink_dyn(b: &mut Bencher, sink_count: usize) {
    b.iter(|| {
        let s = ObsCell::new(0);
        let mut fs = Vec::new();

        for _ in 0..sink_count {
            fs.push(s.as_dyn().fold(0, move |s, x| s + x));
        }
        for i in 0..UPDATE_COUNT {
            s.set(i);
        }

        let mut sum = 0;
        for f in fs {
            sum += f.stop();
        }
        sum
    });
}

fn many_source_sink_inputs() -> Vec<usize> {
    vec![1, 2, 4, 10, 20, 50]
}

fn many_source_sink_ops(b: &mut Bencher, count: usize) {
    b.iter(|| {
        let mut ss = Vec::new();
        for _ in 0..count {
            ss.push(ObsCell::new(0));
        }

        let mut fs = Vec::new();
        for _ in 0..count {
            let f = {
                let ss = ss.clone();
                obs(move |cx| {
                    let mut sum = 0;
                    for s in &ss {
                        sum += s.get(cx)
                    }
                    sum
                })
                .fold(0, |s, x| s + x)
            };
            fs.push(f);
        }

        for i in 0..UPDATE_COUNT {
            ss[i % ss.len()].set(i);
        }
        let mut sum = 0;
        for f in fs {
            sum += f.stop();
        }
        sum
    })
}
fn many_source_sink_dyn(b: &mut Bencher, count: usize) {
    b.iter(|| {
        let mut ss = Vec::new();
        for _ in 0..count {
            ss.push(ObsCell::new(0));
        }

        let mut fs = Vec::new();
        for _ in 0..count {
            let f = {
                let ss = ss.clone();
                DynObs::new(move |cx| {
                    let mut sum = 0;
                    for s in &ss {
                        sum += s.get(cx)
                    }
                    sum
                })
                .fold(0, |s, x| s + x)
            };
            fs.push(f);
        }

        for i in 0..UPDATE_COUNT {
            ss[i % ss.len()].set(i);
        }
        let mut sum = 0;
        for f in fs {
            sum += f.stop();
        }
        sum
    })
}
