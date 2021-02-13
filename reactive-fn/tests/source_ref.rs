use reactive_fn::*;

// #[test]
// fn into_may_obs_int() {
//     fn func_may_re_int(_: impl IntoValueObs<u32>) {}
//     func_may_re_int(10);
//     func_may_re_int(&10);

//     func_may_re_int(DynObs::constant(10));
//     func_may_re_int(&DynObs::constant(10));

//     func_may_re_int(DynObsRef::constant(10));
//     func_may_re_int(&DynObsRef::constant(10));

//     func_may_re_int(DynObsBorrow::constant(10));
//     func_may_re_int(&DynObsBorrow::constant(10));
// }

#[test]
fn into_str() {
    fn func_into(_: impl IntoSourceRef<str>) {}
    func_into("acc");
    func_into(String::from("acb"));

    func_into(DynObs::constant(String::from("abc")));
    func_into(&DynObs::constant(String::from("abc")));
    func_into(DynObsRef::constant(String::from("abc")));
    func_into(&DynObsRef::constant(String::from("abc")));
    func_into(DynObsBorrow::constant(String::from("abc")));
    func_into(&DynObsBorrow::constant(String::from("abc")));

    func_into(obs_constant(String::from("abc")));
    func_into(obs_ref_static("abc"));

    func_into(ObsCell::new(String::from("abc")));
    func_into(&ObsCell::new(String::from("abc")));
}
