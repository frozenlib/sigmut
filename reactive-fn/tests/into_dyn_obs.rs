use reactive_fn::*;

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
