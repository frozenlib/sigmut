use super::*;
use crate::core::Runtime;

#[test]
fn builder_map_and_map_ref() {
    let mut rt = Runtime::new();
    let mut sc = rt.sc();

    let r0 = StateRefBuilder::from_value(10, &mut sc).map(|v| v).build();
    let r1 = StateRefBuilder::from_value(10, &mut sc)
        .map_ref(|v, sc, _| StateRef::from_value(v + 1, sc))
        .build();
    assert_eq!(*r0, 10);
    assert_eq!(*r1, 11);
}
