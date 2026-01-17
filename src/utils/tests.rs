use super::*;

#[test]
fn downcast_and_into_owned() {
    let v = downcast::<String, _>(String::from("ok")).unwrap();
    assert_eq!(v, "ok");
    let err = downcast::<String, _>(10_i32).unwrap_err();
    assert_eq!(err, 10);
    let owned = into_owned("hello");
    assert_eq!(owned, "hello".to_string());
}

#[test]
fn index_new_to_old_apply() {
    let new_to_old = vec![2, 0, 1];
    let idx = IndexNewToOld::new(&new_to_old);
    let old_to_new = idx.build_old_to_new();
    assert_eq!(old_to_new, vec![1, 2, 0]);

    let mut items = vec!['a', 'b', 'c'];
    idx.apply_to(&mut items);
    assert_eq!(items, vec!['c', 'a', 'b']);
    assert_eq!(idx.as_slice(), &[2, 0, 1][..]);
}

#[test]
fn changes_and_ref_count_ops() {
    let mut changes = Changes::new();
    let mut ops = RefCountOps::new();

    ops.increment();
    ops.apply(&mut changes);
    changes.push("a");

    ops.decrement(Some(0));
    ops.apply(&mut changes);

    let mut cleaned = Vec::new();
    changes.clean(|d| cleaned.push(d));
    assert_eq!(cleaned, vec!["a"]);
    let remaining = changes.items(changes.end_age()).next().is_none();
    assert!(remaining);
}

#[test]
fn to_range_variants() {
    assert_eq!(to_range(1..=3, 10), 1..4);
    assert_eq!(to_range(1..3, 10), 1..3);
    assert_eq!(to_range(..3, 10), 0..3);
    assert_eq!(to_range(3.., 10), 3..10);
    assert_eq!(to_range(.., 10), 0..10);
}
