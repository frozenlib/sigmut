use reactive_fn::*;

#[test]
fn into_may_re_int() {
    fn func_may_re_int(_: impl Into<MayRe<u32>>) {}
    func_may_re_int(10);
    func_may_re_int(&10);

    func_may_re_int(Re::constant(10));
    func_may_re_int(&Re::constant(10));

    func_may_re_int(ReRef::constant(10));
    func_may_re_int(&ReRef::constant(10));

    func_may_re_int(ReBorrow::constant(10));
    func_may_re_int(&ReBorrow::constant(10));
}

#[test]
fn into_may_re_ref_str() {
    fn func_may_re_ref_str(_: impl Into<MayReRef<str>>) {}
    func_may_re_ref_str("abc");
    func_may_re_ref_str(String::from("abc"));
    func_may_re_ref_str(Re::constant(String::from("abc")));
    func_may_re_ref_str(&Re::constant(String::from("abc")));
    func_may_re_ref_str(ReRef::constant(String::from("abc")));
    func_may_re_ref_str(&ReRef::constant(String::from("abc")));
    func_may_re_ref_str(ReBorrow::constant(String::from("abc")));
    func_may_re_ref_str(&ReBorrow::constant(String::from("abc")));
}
