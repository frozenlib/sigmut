// `#[allow(named_arguments_used_positionally)]` is effective only when set in crate root.
#![allow(named_arguments_used_positionally)]

use sigmut::{core::Runtime, signal_format};

#[test]
fn use_named_by_index() {
    let mut rt = Runtime::new();
    let s = signal_format!("{}", name = "sigmut");
    assert_eq!(s.get(&mut rt.sc()), "sigmut");
}
