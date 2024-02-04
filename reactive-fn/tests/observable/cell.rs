use reactive_fn::{core::Runtime, helpers::dependency_token::DependencyToken, observable::ObsCell};

#[test]
fn new() {
    let _ = ObsCell::new(10);
}

#[test]
fn notify() {
    let mut rt = Runtime::new();
    let x = ObsCell::new(());
    let t = DependencyToken::new();
    t.update(|oc| x.get(oc.reset()), &mut rt.oc());
    assert!(t.is_up_to_date(&mut rt.uc()));

    x.set((), &mut rt.ac());
    assert!(!t.is_up_to_date(&mut rt.uc()));
}

#[test]
fn set() {
    let mut rt = Runtime::new();
    let x = ObsCell::new(1);
    let c = x.obs().collect_vec();
    rt.update();

    x.set(2, &mut rt.ac());
    rt.update();

    x.set(3, &mut rt.ac());
    rt.update();

    assert_eq!(c.stop(&mut rt.uc()), vec![1, 2, 3]);
}
