use super::*;
use pretty_assertions::assert_eq;

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

#[rstest::rstest]
#[case::empty(&[], &[])]
#[case::singleton(&[7], &[0])]
#[case::sorted(&[1, 2, 3], &[0, 1, 2])]
#[case::reversed(&[3, 2, 1], &[2, 1, 0])]
#[case::equal(&[1, 1, 1], &[0, 1, 2])]
#[case::mixed(&[2, -1, 2, 0, -1], &[1, 4, 3, 0, 2])]
fn index_new_to_old_from_sort(#[case] items: &[i32], #[case] expected: &[usize]) {
    assert_eq!(IndexNewToOld::from_sort(items).as_slice(), expected);
}

#[test]
fn index_new_to_old_from_sort_with_stateful_functions_and_non_clone_items() {
    struct Item(i32);

    let mut key_calls = 0;
    let mut compare_calls = 0;
    let (by_key, by_compare) = {
        let items = [Item(2), Item(1), Item(1), Item(3)];
        let by_key = IndexNewToOld::from_sort_by_key(&items, |item| {
            key_calls += 1;
            item.0
        });
        let by_compare = IndexNewToOld::from_sort_by(&items, |a, b| {
            compare_calls += 1;
            b.0.cmp(&a.0)
        });
        assert_eq!(items.map(|item| item.0), [2, 1, 1, 3]);
        (by_key, by_compare)
    };

    assert!(key_calls > 0);
    assert!(compare_calls > 0);
    assert_eq!(by_key.as_slice(), &[1, 2, 0, 3]);
    assert_eq!(by_compare.as_slice(), &[3, 0, 1, 2]);
}

#[rstest::rstest]
#[case::empty(&[])]
#[case::singleton(&[0])]
#[case::identity(&[0, 1, 2])]
#[case::reverse(&[3, 2, 1, 0])]
#[case::disjoint_cycles(&[1, 0, 3, 2, 4])]
#[case::long_cycle(&[1, 2, 3, 4, 0])]
fn index_new_to_old_applies_permutations(#[case] indices: &[usize]) {
    let order = IndexNewToOld::new(indices);
    let mut items: Vec<_> = (0..indices.len()).collect();
    order.apply_to(&mut items);
    assert_eq!(items, indices);
}

#[test]
fn index_new_to_old_owned_buffer_outlives_borrowed_indices() {
    let order = {
        let indices = vec![2, 0, 1];
        IndexNewToOld::new(&indices).to_owned()
    };
    let borrowed: &IndexNewToOld = order.borrow();
    let mut items = ['a', 'b', 'c'];
    borrowed.apply_to(&mut items);
    assert_eq!(items, ['c', 'a', 'b']);
}

#[rstest::rstest]
#[case::duplicate(&[0, 0, 2])]
#[case::out_of_bounds(&[0, 1, 3])]
#[case::too_short(&[0, 1])]
#[case::too_long(&[0, 1, 2, 3])]
fn index_new_to_old_rejects_invalid_order_before_mutation(#[case] order: &[usize]) {
    let mut items = ['a', 'b', 'c'];
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        IndexNewToOld::new(order).apply_to(&mut items);
    }));

    assert!(result.is_err());
    assert_eq!(items, ['a', 'b', 'c']);
}

#[test]
fn changes_and_ref_count_ops() {
    let mut changes = Changes::new();
    let mut ops = RefCountOps::new();

    ops.increment();
    ops.apply(&mut changes);
    changes.push("a");

    ops.increment_at(0);
    ops.decrement(Some(0));
    ops.apply(&mut changes);

    let mut cleaned = Vec::new();
    changes.clean(|d| cleaned.push(d));
    assert_eq!(cleaned, Vec::<&str>::new());
    assert_eq!(changes.items(0).copied().collect::<Vec<_>>(), vec!["a"]);

    ops.decrement(Some(0));
    ops.apply(&mut changes);
    changes.clean(|d| cleaned.push(d));
    assert_eq!(cleaned, vec!["a"]);
    let remaining = changes.items(changes.end_age()).next().is_none();
    assert!(remaining);
}

#[test]
fn changes_ages_wrap_without_invalidating_cursors() {
    let mut changes = Changes {
        age_base: usize::MAX,
        items: VecDeque::new(),
        end_ref_count: 0,
    };

    changes.push("change");

    assert_eq!(changes.end_age(), 0);
    assert_eq!(
        changes.items(usize::MAX).copied().collect::<Vec<_>>(),
        vec!["change"]
    );
    assert_eq!(
        changes.items(0).copied().collect::<Vec<_>>(),
        Vec::<&str>::new()
    );

    let mut cleaned = Vec::new();
    changes.clean(|change| cleaned.push(change));
    assert_eq!(cleaned, vec!["change"]);
    assert_eq!(changes.end_age(), 0);
}

#[test]
fn to_range_variants() {
    assert_eq!(to_range(1..=3, 10), 1..4);
    assert_eq!(to_range(1..3, 10), 1..3);
    assert_eq!(to_range(..3, 10), 0..3);
    assert_eq!(to_range(3.., 10), 3..10);
    assert_eq!(to_range(.., 10), 0..10);
}
