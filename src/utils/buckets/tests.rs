use super::*;
use pretty_assertions::assert_eq;

#[test]
fn ids_returns_non_empty_bucket_ids() {
    let mut buckets = Buckets::new();
    assert_eq!(buckets.ids().collect::<Vec<_>>(), Vec::<isize>::new());

    buckets.push(-2, 0);
    buckets.push(0, 1);
    buckets.push(2, 2);
    buckets.push(2, 3);
    assert_eq!(buckets.ids().collect::<Vec<_>>(), [-2, 0, 2]);

    assert_eq!(buckets.pop_front(0), Some(1));
    assert_eq!(buckets.ids().collect::<Vec<_>>(), [-2, 2]);

    let mut drained = Vec::new();
    buckets.drain(Some(2), &mut drained);
    assert_eq!(drained, [2, 3]);
    assert_eq!(buckets.ids().collect::<Vec<_>>(), [-2]);

    buckets.drain(None, &mut drained);
    assert_eq!(buckets.ids().collect::<Vec<_>>(), Vec::<isize>::new());
}
