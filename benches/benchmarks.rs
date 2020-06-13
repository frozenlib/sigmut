use criterion::{criterion_group, criterion_main, Criterion};
use reactive_fn::*;

criterion_main!(benches);
criterion_group!(benches, criterion_benchmark);

pub fn criterion_benchmark(c: &mut Criterion) {
    {
        let mut g = c.benchmark_group("re_fold 10000");
        g.bench_function("dyn", |b| b.iter(|| re_fold_dyn(10000)));
        g.bench_function("ops", |b| b.iter(|| re_fold_ops(10000)));
    }
    {
        let mut g = c.benchmark_group("re_map_fold 10000");
        g.bench_function("dyn", |b| b.iter(|| re_map_fold_dyn(10000)));
        g.bench_function("ops", |b| b.iter(|| re_map_fold_ops(10000)));
    }
    {
        let mut g = c.benchmark_group("re_cell_flatten 1000");
        g.bench_function("dyn", |b| b.iter(|| re_cell_flatten_dyn(1000)));
        g.bench_function("ops", |b| b.iter(|| re_cell_flatten_ops(1000)));
    }
    {
        let mut g = c.benchmark_group("many_source 100 100");
        g.bench_function("dyn", |b| b.iter(|| many_source_dyn(100, 100)));
        g.bench_function("ops", |b| b.iter(|| many_source_ops(100, 100)));
    }
    {
        let mut g = c.benchmark_group("many_sink 100 100");
        g.bench_function("dyn", |b| b.iter(|| many_sink_dyn(100, 100)));
        g.bench_function("ops", |b| b.iter(|| many_sink_ops(100, 100)));
    }
    {
        let mut g = c.benchmark_group("many_source_sink 30 30 30");
        g.bench_function("dyn", |b| b.iter(|| many_source_sink_dyn(30, 30, 30)));
        g.bench_function("ops", |b| b.iter(|| many_source_sink_ops(30, 30, 30)));
    }
}

fn re_fold_dyn(n: usize) -> usize {
    let cell = ReCell::new(0);
    let fold = cell.to_re().fold(0, |s, x| s + x);

    for i in 1..n {
        cell.set_and_update(i);
    }
    fold.stop()
}
fn re_fold_ops(n: usize) -> usize {
    let cell = ReCell::new(0);
    let fold = cell.ops().fold(0, |s, x| s + x);

    for i in 1..n {
        cell.set_and_update(i);
    }
    fold.stop()
}

fn re_map_fold_dyn(n: usize) -> usize {
    let cell = ReCell::new(0);
    let fold = cell.to_re().map(|x| x * 2).fold(0, |s, x| s + x);

    for i in 1..n {
        cell.set_and_update(i);
    }
    fold.stop()
}
fn re_map_fold_ops(n: usize) -> usize {
    let cell = ReCell::new(0);
    let fold = cell.ops().map(|x| x * 2).fold(0, |s, x| s + x);

    for i in 1..n {
        cell.set_and_update(i);
    }
    fold.stop()
}

fn re_cell_flatten_dyn(n: usize) -> usize {
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

fn re_cell_flatten_ops(n: usize) -> usize {
    let s = ReRefCell::new(Re::constant(0));
    let s1 = Re::constant(1);
    let s2 = Re::constant(2);
    let f = s.ops().flatten().fold(0, |s, x| s + x);

    for _ in 0..n {
        s.set_and_update(s1.clone());
        s.set_and_update(s2.clone());
    }
    f.stop()
}

fn many_source_dyn(source_count: usize, repeat: usize) -> usize {
    let mut ss = Vec::new();
    for _ in 0..source_count {
        ss.push(ReCell::new(0));
    }

    let f = {
        let ss = ss.clone();
        Re::new(move |ctx| {
            let mut sum = 0;
            for s in &ss {
                sum += s.get(ctx)
            }
            sum
        })
        .fold(0, |s, x| s + x)
    };

    for i in 0..repeat {
        ss[i % source_count].set_and_update(i);
    }
    f.stop()
}
fn many_source_ops(source_count: usize, repeat: usize) -> usize {
    let mut ss = Vec::new();
    for _ in 0..source_count {
        ss.push(ReCell::new(0));
    }

    let f = {
        let ss = ss.clone();
        re(move |ctx| {
            let mut sum = 0;
            for s in &ss {
                sum += s.get(ctx)
            }
            sum
        })
        .fold(0, |s, x| s + x)
    };

    for i in 0..repeat {
        ss[i % source_count].set_and_update(i);
    }
    f.stop()
}

fn many_sink_dyn(sink_count: usize, repeat: usize) -> usize {
    let s = ReCell::new(0);
    let mut fs = Vec::new();

    for _ in 0..sink_count {
        fs.push(s.to_re().fold(0, move |s, x| s + x));
    }
    for i in 0..repeat {
        s.set_and_update(i);
    }

    let mut sum = 0;
    for f in fs {
        sum += f.stop();
    }
    sum
}
fn many_sink_ops(sink_count: usize, repeat: usize) -> usize {
    let s = ReCell::new(0);
    let mut fs = Vec::new();

    for _ in 0..sink_count {
        fs.push(s.ops().fold(0, move |s, x| s + x));
    }
    for i in 0..repeat {
        s.set_and_update(i);
    }

    let mut sum = 0;
    for f in fs {
        sum += f.stop();
    }
    sum
}

fn many_source_sink_dyn(source_count: usize, sink_count: usize, repeat: usize) -> usize {
    let mut ss = Vec::new();
    for _ in 0..source_count {
        ss.push(ReCell::new(0));
    }

    let mut fs = Vec::new();
    for _ in 0..sink_count {
        let f = {
            let ss = ss.clone();
            Re::new(move |ctx| {
                let mut sum = 0;
                for s in &ss {
                    sum += s.get(ctx)
                }
                sum
            })
            .fold(0, |s, x| s + x)
        };
        fs.push(f);
    }

    for i in 0..repeat {
        let len = ss.len();
        ss[i % len].set_and_update(i);
    }
    let mut sum = 0;
    for f in fs {
        sum += f.stop();
    }
    sum
}

fn many_source_sink_ops(source_count: usize, sink_count: usize, repeat: usize) -> usize {
    let mut ss = Vec::new();
    for _ in 0..source_count {
        ss.push(ReCell::new(0));
    }

    let mut fs = Vec::new();
    for _ in 0..sink_count {
        let f = {
            let ss = ss.clone();
            re(move |ctx| {
                let mut sum = 0;
                for s in &ss {
                    sum += s.get(ctx)
                }
                sum
            })
            .fold(0, |s, x| s + x)
        };
        fs.push(f);
    }

    for i in 0..repeat {
        let len = ss.len();
        ss[i % len].set_and_update(i);
    }
    let mut sum = 0;
    for f in fs {
        sum += f.stop();
    }
    sum
}
