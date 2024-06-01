use sigmut::{
    collections::vec::{StateVec, VecChange},
    SignalBuilder,
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

    rt.update(); // prints "0"
    {
        let mut items = s.items_mut(rt.ac());
        items.push(1);
        items.push(2);
    }
    rt.update(); // prints "3"
}
