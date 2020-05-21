mod test_utils;

use self::test_utils::*;
use reactive_fn::*;

#[test]
fn re_ref_constant() {
    let a = ReRef::constant(2);
    let r = record(&a.cloned());
    assert_eq!(r.finish(), vec![2]);
}
