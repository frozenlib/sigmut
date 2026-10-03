use sigmut::{Signal, SignalBuilder};

// The runner replaces MAP_STEPS with a repeated map / map_value chain.
// Keeping all stages in one builder deliberately stresses recursive type names.
macro_rules! chain {
    ($builder:expr) => {
        $builder /* MAP_STEPS */
    };
}

fn get<const ID: usize>(source: Signal<i64>) -> Signal<i64> {
    chain!(SignalBuilder::new(move |sc| source.get(sc) + ID as i64)).build()
}

fn scan<const ID: usize>(source: Signal<i64>) -> Signal<i64> {
    chain!(
        SignalBuilder::from_scan_filter(0, move |state, sc| {
            let next = source.get(sc) + ID as i64;
            let dirty = *state != next;
            *state = next;
            dirty
        })
        .on_discard(|state| *state = 0)
    )
    .build()
}

pub fn signals(source: Signal<i64>) -> Vec<Signal<i64>> {
    vec![
        get::<0>(source.clone()),
        get::<1>(source.clone()),
        get::<2>(source.clone()),
        get::<3>(source.clone()),
        scan::<0>(source.clone()),
        scan::<1>(source.clone()),
        scan::<2>(source.clone()),
        scan::<3>(source),
    ]
}
