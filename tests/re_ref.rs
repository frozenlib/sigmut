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

#[test]
fn re_ref_new_cell2() {
    let cell1 = ReCell::new(1);
    let cell2 = ReCell::new(2);

    let re = ReRef::new(
        (cell1.clone(), cell2.clone()),
        move |(cell1, cell2), ctx, f| f(&(cell1.get(ctx) + cell2.get(ctx))),
    )
    .cloned();
    let r = record(&re);

    cell1.set_and_update(5);
    cell2.set_and_update(10);

    assert_eq!(r.finish(), vec![1 + 2, 5 + 2, 5 + 10]);
}

#[test]
fn re_ref_fold() {
    let cell = ReRefCell::new(1);
    let fold = cell.to_re_ref().fold(2, |s, x| s + x);

    cell.set_and_update(5);
    cell.set_and_update(10);

    assert_eq!(fold.stop(), 18);
}

#[test]
fn re_ref_to_vec() {
    let cell = ReRefCell::new(1);
    let fold = cell.to_re_ref().to_vec();

    cell.set_and_update(2);
    cell.set_and_update(1);
    cell.set_and_update(3);

    assert_eq!(fold.stop(), vec![1, 2, 1, 3]);
}
