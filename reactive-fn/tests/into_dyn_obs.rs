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
fn into_dyn_str() {
    fn func_into_dyn_str(_: impl IntoDynObsRef<str>) {}
    func_into_dyn_str(DynObs::constant(String::from("abc")));
    func_into_dyn_str(&DynObs::constant(String::from("abc")));
    func_into_dyn_str(DynObsRef::constant(String::from("abc")));
    func_into_dyn_str(&DynObsRef::constant(String::from("abc")));
    func_into_dyn_str(DynObsBorrow::constant(String::from("abc")));
    func_into_dyn_str(&DynObsBorrow::constant(String::from("abc")));
}
