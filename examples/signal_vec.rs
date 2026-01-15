use sigmut::{
    SignalBuilder,
    collections::vec::{StateVec, VecChange},
};

fn main() {
    let mut rt = sigmut::core::Runtime::new();

    let s = StateVec::new();

    let mut r = s.to_signal_vec().reader();
    let _e = SignalBuilder::from_scan(0, move |sum, sc| {
        for change in r.read(sc).changes() {
            match change {
                VecChange::Insert { new_value, .. } => *sum += new_value,
                VecChange::Remove { old_value, .. } => *sum -= old_value,
                VecChange::Set {
                    new_value,
                    old_value,
                    ..
                } => {
                    *sum -= old_value;
                    *sum += new_value;
                }
                VecChange::Move { .. } | VecChange::Swap { .. } | VecChange::Sort(_) => {}
            }
        }
    })
    .build()
    .effect(|sum| println!("{sum}"));

    rt.flush(); // prints "0"

    s.borrow_mut(rt.ac()).extend([1, 2]);
    rt.flush(); // prints "3"
}
