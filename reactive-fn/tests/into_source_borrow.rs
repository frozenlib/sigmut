use reactive_fn::*;

#[test]
fn into_source_borrow_str() {
    fn func_into(_: impl IntoSourceBorrow<str>) {}
    func_into("acc");
    func_into(String::from("acb"));

    func_into(DynObs::new_constant(String::from("abc")));
    func_into(&DynObs::new_constant(String::from("abc")));

    func_into(obs_constant(String::from("abc")));
    func_into(obs_static("abc"));

    func_into(ObsCell::new(String::from("abc")));
    func_into(&ObsCell::new(String::from("abc")));
}
