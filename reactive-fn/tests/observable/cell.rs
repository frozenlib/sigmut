use reactive_fn::{
    core::{dependency_token::DependencyToken, Runtime},
    observable::ObsCell,
};

#[test]
fn new() {
    let _ = ObsCell::new(10);
}

#[test]
fn notify() {
    let mut dc = Runtime::new();
    let x = ObsCell::new(());
    let t = DependencyToken::new();
    t.update(|cc| x.get(cc.oc()), dc.ac().oc());
    assert!(t.is_up_to_date(dc.uc()));

    x.set((), &mut dc.ac());
    assert!(!t.is_up_to_date(dc.uc()));
}

#[test]
fn set() {
    let mut dc = Runtime::new();
    let x = ObsCell::new(1);
    let c = x.obs().collect_vec();
    dc.update();

    x.set(2, &mut dc.ac());
    dc.update();

    x.set(3, &mut dc.ac());
    dc.update();

    assert_eq!(c.stop(dc.uc()), vec![1, 2, 3]);
}
