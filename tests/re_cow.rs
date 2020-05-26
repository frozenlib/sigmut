use reactive_fn::*;

#[test]
fn into_re_cow_int() {
    fn func_re_cow_int(_: impl Into<ReCow<u32>>) {}
    func_re_cow_int(10);
    func_re_cow_int(&10);

    func_re_cow_int(Re::constant(10));
    func_re_cow_int(&Re::constant(10));

    func_re_cow_int(ReRef::constant(10));
    func_re_cow_int(&ReRef::constant(10));

    func_re_cow_int(ReBorrow::constant(10));
    func_re_cow_int(&ReBorrow::constant(10));
}

#[test]
fn into_re_cow_str() {
    fn func_re_cow_str(_: impl Into<ReCow<str>>) {}
    func_re_cow_str("abc");
    func_re_cow_str(String::from("abc"));
    func_re_cow_str(Re::constant(String::from("abc")));
    func_re_cow_str(&Re::constant(String::from("abc")));
    func_re_cow_str(ReRef::constant(String::from("abc")));
    func_re_cow_str(&ReRef::constant(String::from("abc")));
    func_re_cow_str(ReBorrow::constant(String::from("abc")));
    func_re_cow_str(&ReBorrow::constant(String::from("abc")));
}
