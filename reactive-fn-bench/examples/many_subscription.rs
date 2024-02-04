#[cfg(not(unix))]
fn main() {
    use reactive_fn::{core::Runtime, ObsCell};
    use std::time::Instant;

    const PHASE: usize = 100;
    const COUNT: usize = 1000;
    const SUBSCRIPTIONS: usize = 10000;

    let start = Instant::now();
    for _ in 0..PHASE {
        let cell = ObsCell::new(0);
        let mut rt = Runtime::new();
        let mut ss = Vec::new();
        for _ in 0..SUBSCRIPTIONS {
            ss.push(cell.obs_builder().map(|x| x + 1).subscribe(|_| {}));
        }
        for i in 0..COUNT {
            cell.set(i, &mut rt.ac());
            rt.update();
        }
    }
    let end = Instant::now();
    println!("{}", (end - start).as_secs_f64());
}

#[cfg(unix)]
fn main() {
    use reactive_fn::{core::Runtime, ObsCell};
    use std::fs::File;

    const COUNT: usize = 100;
    const SUBSCRIPTIONS: usize = 10000;

    let guard = pprof::ProfilerGuardBuilder::default()
        .frequency(1000)
        .blocklist(&["libc", "libgcc", "pthread", "vdso"])
        .build()
        .unwrap();

    let cell = ObsCell::new(0);
    for _ in 0..1000 {
        let mut rt = Runtime::new();
        let mut ss = Vec::new();
        for _ in 0..SUBSCRIPTIONS {
            ss.push(cell.obs_builder().map(|x| x + 1).subscribe(|_| {}));
        }
        for i in 0..COUNT {
            cell.set(i, &mut rt.ac());
            rt.update();
        }
    }

    if let Ok(report) = guard.report().build() {
        let file = File::create("flamegraph.svg").unwrap();
        report.flamegraph(file).unwrap();
    };
}
