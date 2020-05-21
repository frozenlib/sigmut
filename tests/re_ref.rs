mod test_utils;

use self::test_utils::*;
use reactive_fn::*;

#[test]
fn re_ref_constant() {
    let a = ReRef::constant(2);
    let r = record(&a.cloned());
    assert_eq!(r.finish(), vec![2]);
}
#[test]
fn re_ref_new() {
    let a = ReCell::new(2);
    let b = ReRef::new(a.clone(), move |a, ctx, f| f(&a.get(ctx)));
    let r = record(&b.cloned());

    a.set_and_update(5);
    a.set_and_update(7);

    assert_eq!(r.finish(), vec![2, 5, 7]);
}
