use reactive_fn::*;
use std::collections::HashSet;

#[test]
fn constant() {
    let r = obs_constant(2).collect_vec();
    assert_eq!(r.stop(), vec![2]);
}

#[test]
fn new() {
    let a = ObsCell::new(2);
    let a_ = a.clone();
    let r = obs(move |bc| a_.get(bc)).collect_vec();

    a.set(5);
    a.set(7);

    assert_eq!(r.stop(), vec![2, 5, 7]);
}

#[test]
fn new_cell2() {
    let cell1 = ObsCell::new(1);
    let cell2 = ObsCell::new(2);

    let r = {
        let cell1 = cell1.clone();
        let cell2 = cell2.clone();
        obs(move |bc| cell1.get(bc) + cell2.get(bc)).collect_vec()
    };

    cell1.set(5);
    cell2.set(10);

    assert_eq!(r.stop(), vec![1 + 2, 5 + 2, 5 + 10]);
}

#[test]
fn map() {
    let a = ObsCell::new(2);
    let r = a.obs().map(|x| x * 2).collect_vec();

    a.set(5);
    a.set(7);

    assert_eq!(r.stop(), vec![4, 10, 14]);
}

#[test]
fn flat_map() {
    let a = [ObsCell::new(5), ObsCell::new(10)];
    let a_ = a.clone();

    let b = ObsCell::new(0);
    let r = b.obs().flat_map(move |&x| a_[x].obs()).collect_vec();

    a[0].set(6);
    a[1].set(12);

    a[0].set(7);
    a[1].set(13);

    b.set(1);

    a[0].set(8);
    a[1].set(14);

    assert_eq!(r.stop(), vec![5, 6, 7, 13, 14]);
}

#[test]
fn cached() {
    let cell = ObsCell::new(0);
    let r = cell.obs().map(|x| x + 1).cached().collect_vec();

    cell.set(5);
    cell.set(10);

    assert_eq!(r.stop(), vec![1, 6, 11]);
}

#[test]
fn scan() {
    let cell = ObsCell::new(2);
    let r = cell.obs().scan(10, |s, x| *s += x).collect_vec();

    cell.set(3);
    cell.set(4);
    cell.set(5);

    assert_eq!(r.stop(), vec![12, 15, 19, 24]);
}
#[test]
fn filter_scan() {
    let cell = ObsCell::new(2);
    let r = cell
        .obs()
        .filter_scan(10, |_s, x| x % 2 != 0, |s, x| *s += x)
        .collect_vec();

    cell.set(3);
    cell.set(4);
    cell.set(5);
    cell.set(6);

    assert_eq!(r.stop(), vec![10, 13, 18]);
}

#[test]
fn same_value() {
    let cell = ObsCell::new(5);
    let r = cell.obs().collect_vec();

    cell.set(5);
    cell.set(5);

    assert_eq!(r.stop(), vec![5, 5, 5]);
}
#[test]
fn dedup() {
    let cell = ObsCell::new(5);
    let r = cell.obs().dedup().collect_vec();

    cell.set(5);
    cell.set(5);
    cell.set(6);
    cell.set(6);
    cell.set(5);

    assert_eq!(r.stop(), vec![5, 6, 5]);
}

#[test]
fn dedup_by_key_1() {
    let cell = ObsCell::new((5, 1));
    let r = cell.obs().dedup_by_key(|&(x, _)| x).collect_vec();

    cell.set((5, 2));
    cell.set((6, 2));
    cell.set((6, 2));
    cell.set((6, 1));
    cell.set((5, 2));

    assert_eq!(r.stop(), vec![(5, 1), (6, 2), (5, 2)]);
}

#[test]
fn dedup_by_key_2() {
    let cell = ObsCell::new((5, 1));
    let obs = cell.obs().dedup_by_key(|&(x, _)| x);

    cell.set((5, 2));
    let r = obs.collect_vec(); // current value is (5, 2), not (5, 1).
    cell.set((6, 2));
    cell.set((6, 2));
    cell.set((6, 1));
    cell.set((5, 2));

    assert_eq!(r.stop(), vec![(5, 2), (6, 2), (5, 2)]);
}

#[test]
fn dedup_by() {
    let cell = ObsCell::new((5, 1));
    let r = cell
        .obs()
        .dedup_by(|&(x1, _), &(x2, _)| x1 == x2)
        .collect_vec();

    cell.set((5, 2));
    cell.set((6, 2));
    cell.set((6, 2));
    cell.set((6, 1));
    cell.set((5, 2));

    assert_eq!(r.stop(), vec![(5, 1), (6, 2), (5, 2)]);
}

#[test]
fn fold() {
    let cell = ObsCell::new(1);
    let fold = cell.obs().fold(2, |s, x| *s += x);

    cell.set(5);
    cell.set(10);

    assert_eq!(fold.stop(), 18);
}
#[test]
fn collect_to() {
    let cell = ObsCell::new(1);
    let fold = cell.obs().collect_to(HashSet::new());

    cell.set(2);
    cell.set(1);
    cell.set(3);

    let e: HashSet<_> = vec![1, 2, 3].into_iter().collect();
    assert_eq!(fold.stop(), e);
}
#[test]
fn collect() {
    let cell = ObsCell::new(1);
    let fold = cell.obs().collect_to(HashSet::new());

    cell.set(2);
    cell.set(1);
    cell.set(3);

    let e: HashSet<_> = vec![1, 2, 3].into_iter().collect();
    let a: HashSet<_> = fold.stop();
    assert_eq!(a, e);
}

#[test]
fn collect_vec() {
    let cell = ObsCell::new(1);
    let fold = cell.obs().collect_vec();

    cell.set(2);
    cell.set(1);
    cell.set(3);

    assert_eq!(fold.stop(), vec![1, 2, 1, 3]);
}

#[test]
fn subscribe() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let cell = ObsCell::new(0);
    let vs = Rc::new(RefCell::new(Vec::new()));

    let vs_send = vs.clone();

    let r = cell.obs().subscribe(move |&x| {
        vs_send.borrow_mut().push(x);
    });

    cell.set(5);
    cell.set(10);

    drop(r);
    assert_eq!(*vs.borrow(), vec![0, 5, 10]);

    cell.set(15);
    assert_eq!(*vs.borrow(), vec![0, 5, 10]);
}

#[test]
fn hot() {
    let cell = ObsCell::new(1);
    let obs = cell.obs().scan(0, |s, x| *s += x);

    let hot = obs.hot();

    cell.set(2);
    cell.set(10);

    assert_eq!(hot.collect_vec().stop(), vec![13]);
}

#[test]
fn hot_no() {
    let cell = ObsCell::new(1);
    let obs = cell.obs().scan(0, |s, x| *s += x);

    cell.set(2);
    cell.set(10);

    assert_eq!(obs.collect_vec().stop(), vec![10]);
}

#[test]
fn flatten() {
    let cell = ObsCell::new(obs_constant(1));

    let vs = cell.as_dyn().flatten().collect_vec();

    cell.set(obs_constant(2));
    cell.set(obs_constant(3));
    cell.set(obs_constant(4));
    cell.set(obs_constant(5));

    assert_eq!(vs.stop(), vec![1, 2, 3, 4, 5]);
}

#[test]
fn get_head_tail() {
    let a = ObsCell::new(2);
    let (head, tail) = a.obs().get_head_tail();
    let r = tail.collect_vec();

    a.set(5);
    a.set(7);

    assert_eq!(head, 2);
    assert_eq!(r.stop(), vec![5, 7]);
}

#[test]
fn get_head_tail_after_set() {
    let a = ObsCell::new(2);
    let (head, tail) = a.obs().get_head_tail();

    a.set(5);
    let r = tail.collect_vec();
    a.set(7);

    assert_eq!(head, 2);
    assert_eq!(r.stop(), vec![5, 7]);
}
