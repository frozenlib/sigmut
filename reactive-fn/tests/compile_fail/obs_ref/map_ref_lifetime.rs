use reactive_fn::core::{ObsRef, ObsRefBuilder, Runtime};

fn main() {
    struct X;

    let mut rt = Runtime::new();
    let oc = &mut rt.oc();
    let value = X;
    let or = ObsRefBuilder::from_value_non_static(&value, oc)
        .map_ref(|_, oc, _| ObsRef::from_value_non_static(10, oc))
        .build();
    drop(value);
    drop(or);
}
