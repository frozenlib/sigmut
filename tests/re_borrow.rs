use reactive_fn::*;

#[test]
fn re_borrow_constant() {
    let r = ReBorrow::constant(2).to_vec();
    assert_eq!(r.stop(), vec![2]);
}
#[test]
fn re_borrow_new() {
    let a = ReRefCell::new(2);
    let r = ReBorrow::new(a.clone(), move |a, ctx| a.borrow(ctx)).to_vec();

    a.set_and_update(5);
    a.set_and_update(7);

    assert_eq!(r.stop(), vec![2, 5, 7]);
}
