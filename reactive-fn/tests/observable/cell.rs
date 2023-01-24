use reactive_fn::{
    core::{dependency_token::DependencyToken, DependencyContext},
    observable::ObsCell,
};

#[test]
fn new() {
    let _ = ObsCell::new(10);
}

#[test]
fn notify() {
    DependencyContext::with(|dc| {
        let x = ObsCell::new(());
        let t = DependencyToken::new();
        t.update(|cc| x.get(cc.oc()), dc.ac().oc());
        assert!(t.is_up_to_date(dc.ac().oc()));

        x.set((), &mut dc.ac());
        assert!(!t.is_up_to_date(dc.ac().oc()));
    });
}

#[test]
fn set() {
    DependencyContext::with(|dc| {
        let x = ObsCell::new(1);
        let c = x.obs().collect_vec();
        dc.update();

        x.set(2, &mut dc.ac());
        dc.update();

        x.set(3, &mut dc.ac());
        dc.update();

        assert_eq!(c.stop(dc.ac().oc()), vec![1, 2, 3]);
    });
}
