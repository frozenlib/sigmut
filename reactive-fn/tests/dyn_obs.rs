use ::rt_local::runtime::core::test;
use ::rt_local::wait_for_idle;
use reactive_fn::*;
use std::collections::HashSet;

#[test]
async fn map() {
    let a = ObsCell::new(2);
    let r = a.as_dyn().map(|x| x * 2).collect_vec();
    wait_for_idle().await;

    a.set(5);
    wait_for_idle().await;

    a.set(7);
    wait_for_idle().await;

    assert_eq!(r.stop(), vec![4, 10, 14]);
}

#[test]
async fn flat_map() {
    let a = [ObsCell::new(5), ObsCell::new(10)];
    let a_ = a.clone();
    let b = ObsCell::new(0);
    let r = b.as_dyn().flat_map(move |&x| a_[x].as_dyn()).collect_vec();
    wait_for_idle().await;

    a[0].set(6);
    a[1].set(12);
    wait_for_idle().await;

    a[0].set(7);
    a[1].set(13);
    wait_for_idle().await;

    b.set(1);
    wait_for_idle().await;

    a[0].set(8);
    a[1].set(14);
    wait_for_idle().await;

    assert_eq!(r.stop(), vec![5, 6, 7, 13, 14]);
}

#[test]
async fn cahced() {
    let cell = ObsCell::new(0);
    let r = cell.as_dyn().map(|x| x + 1).cached().collect_vec();
    wait_for_idle().await;

    cell.set(5);
    wait_for_idle().await;

    cell.set(10);
    wait_for_idle().await;

    assert_eq!(r.stop(), vec![1, 6, 11]);
}

#[test]
async fn scan() {
    let cell = ObsCell::new(2);
    let r = cell.as_dyn().scan(10, |s, x| *s += x).collect_vec();
    wait_for_idle().await;

    cell.set(3);
    wait_for_idle().await;

    cell.set(4);
    wait_for_idle().await;

    cell.set(5);
    wait_for_idle().await;

    assert_eq!(r.stop(), vec![12, 15, 19, 24]);
}
#[test]
async fn filter_scan() {
    let cell = ObsCell::new(2);
    let r = cell
        .as_dyn()
        .filter_scan(10, |_s, x| x % 2 != 0, |s, x| *s += x)
        .collect_vec();
    wait_for_idle().await;

    cell.set(3);
    wait_for_idle().await;

    cell.set(4);
    wait_for_idle().await;

    cell.set(5);
    wait_for_idle().await;

    cell.set(6);
    wait_for_idle().await;

    assert_eq!(r.stop(), vec![10, 13, 18]);
}

#[test]
async fn same_value() {
    let cell = ObsCell::new(5);
    let r = cell.as_dyn().collect_vec();
    wait_for_idle().await;

    cell.set(5);
    wait_for_idle().await;

    cell.set(5);
    wait_for_idle().await;

    assert_eq!(r.stop(), vec![5, 5, 5]);
}

#[test]
async fn dedup() {
    let cell = ObsCell::new(5);
    let r = cell.as_dyn().dedup().collect_vec();
    wait_for_idle().await;

    cell.set(5);
    wait_for_idle().await;

    cell.set(5);
    wait_for_idle().await;

    cell.set(6);
    wait_for_idle().await;

    cell.set(6);
    wait_for_idle().await;

    cell.set(5);
    wait_for_idle().await;

    assert_eq!(r.stop(), vec![5, 6, 5]);
}

#[test]
async fn re_dedup_by_key_1() {
    let cell = ObsCell::new((5, 1));
    let r = cell.as_dyn().dedup_by_key(|&(x, _)| x).collect_vec();
    wait_for_idle().await;

    cell.set((5, 2));
    wait_for_idle().await;

    cell.set((6, 2));
    wait_for_idle().await;

    cell.set((6, 2));
    wait_for_idle().await;

    cell.set((6, 1));
    wait_for_idle().await;

    cell.set((5, 2));
    wait_for_idle().await;

    assert_eq!(r.stop(), vec![(5, 1), (6, 2), (5, 2)]);
}

#[test]
async fn re_dedup_by_key_2() {
    let cell = ObsCell::new((5, 1));
    let obs = cell.as_dyn().dedup_by_key(|&(x, _)| x);
    wait_for_idle().await;

    cell.set((5, 2));
    wait_for_idle().await;

    let r = obs.collect_vec(); // current value is (5, 2), not (5, 1).
    wait_for_idle().await;

    cell.set((6, 2));
    wait_for_idle().await;

    cell.set((6, 2));
    wait_for_idle().await;

    cell.set((6, 1));
    wait_for_idle().await;

    cell.set((5, 2));
    wait_for_idle().await;

    assert_eq!(r.stop(), vec![(5, 2), (6, 2), (5, 2)]);
}

#[test]
async fn dedup_by() {
    let cell = ObsCell::new((5, 1));
    let r = cell
        .as_dyn()
        .dedup_by(|&(x1, _), &(x2, _)| x1 == x2)
        .collect_vec();
    wait_for_idle().await;

    cell.set((5, 2));
    wait_for_idle().await;

    cell.set((6, 2));
    wait_for_idle().await;

    cell.set((6, 2));
    wait_for_idle().await;

    cell.set((6, 1));
    wait_for_idle().await;

    cell.set((5, 2));
    wait_for_idle().await;

    assert_eq!(r.stop(), vec![(5, 1), (6, 2), (5, 2)]);
}

#[test]
async fn fold() {
    let cell = ObsCell::new(1);
    let fold = cell.as_dyn().fold(2, |s, x| *s += x);
    wait_for_idle().await;

    cell.set(5);
    wait_for_idle().await;

    cell.set(10);
    wait_for_idle().await;

    assert_eq!(fold.stop(), 18);
}
#[test]
async fn collect_to() {
    let cell = ObsCell::new(1);
    let fold = cell.as_dyn().collect_to(HashSet::new());
    wait_for_idle().await;

    cell.set(2);
    wait_for_idle().await;

    cell.set(1);
    wait_for_idle().await;

    cell.set(3);
    wait_for_idle().await;

    let e: HashSet<_> = vec![1, 2, 3].into_iter().collect();
    assert_eq!(fold.stop(), e);
}
#[test]
async fn collect() {
    let cell = ObsCell::new(1);
    let fold = cell.as_dyn().collect_to(HashSet::new());
    wait_for_idle().await;

    cell.set(2);
    wait_for_idle().await;

    cell.set(1);
    wait_for_idle().await;

    cell.set(3);
    wait_for_idle().await;

    let e: HashSet<_> = vec![1, 2, 3].into_iter().collect();
    let a: HashSet<_> = fold.stop();
    assert_eq!(a, e);
}

#[test]
async fn collect_vec() {
    let cell = ObsCell::new(1);
    let fold = cell.as_dyn().collect_vec();
    wait_for_idle().await;

    cell.set(2);
    wait_for_idle().await;

    cell.set(1);
    wait_for_idle().await;

    cell.set(3);
    wait_for_idle().await;

    assert_eq!(fold.stop(), vec![1, 2, 1, 3]);
}

#[test]
async fn subscribe() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let cell = ObsCell::new(0);
    let vs = Rc::new(RefCell::new(Vec::new()));
    let vs_send = vs.clone();
    let r = cell.as_dyn().subscribe(move |&x| {
        vs_send.borrow_mut().push(x);
    });
    wait_for_idle().await;

    cell.set(5);
    wait_for_idle().await;

    cell.set(10);
    wait_for_idle().await;

    drop(r);
    assert_eq!(*vs.borrow(), vec![0, 5, 10]);

    cell.set(15);
    assert_eq!(*vs.borrow(), vec![0, 5, 10]);
}

#[test]
async fn hot() {
    let cell = ObsCell::new(1);
    let obs = cell.as_dyn().scan(0, |s, x| *s += x);
    let hot = obs.hot();
    wait_for_idle().await;

    cell.set(2);
    wait_for_idle().await;

    cell.set(10);
    wait_for_idle().await;

    assert_eq!(hot.collect_vec().stop(), vec![13]);
}

#[test]
async fn hot_no() {
    let cell = ObsCell::new(1);
    let obs = cell.as_dyn().scan(0, |s, x| *s += x);
    wait_for_idle().await;

    cell.set(2);
    wait_for_idle().await;

    cell.set(10);
    wait_for_idle().await;

    assert_eq!(obs.collect_vec().stop(), vec![10]);
}

#[test]
async fn flatten() {
    let cell = ObsCell::new(obs_constant(1));
    let vs = cell.as_dyn().flatten().collect_vec();
    wait_for_idle().await;

    cell.set(obs_constant(2));
    wait_for_idle().await;

    cell.set(obs_constant(3));
    wait_for_idle().await;

    cell.set(obs_constant(4));
    wait_for_idle().await;

    cell.set(obs_constant(5));
    wait_for_idle().await;

    assert_eq!(vs.stop(), vec![1, 2, 3, 4, 5]);
}

#[test]
async fn get_head_tail() {
    let a = ObsCell::new(2);
    let (head, tail) = a.as_dyn().get_head_tail();
    let r = tail.collect_vec();
    wait_for_idle().await;

    a.set(5);
    wait_for_idle().await;

    a.set(7);
    wait_for_idle().await;

    assert_eq!(head, 2);
    assert_eq!(r.stop(), vec![5, 7]);
}

#[test]
async fn get_head_tail_after_set() {
    let a = ObsCell::new(2);
    let (head, tail) = a.as_dyn().get_head_tail();
    wait_for_idle().await;

    a.set(5);
    wait_for_idle().await;

    let r = tail.collect_vec();
    wait_for_idle().await;

    a.set(7);
    wait_for_idle().await;

    assert_eq!(head, 2);
    assert_eq!(r.stop(), vec![5, 7]);
}
