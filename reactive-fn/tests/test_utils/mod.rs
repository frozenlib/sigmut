use reactive_fn::core::Runtime;

pub mod code_path;

pub fn dc_test(f: impl FnOnce(&mut Runtime)) {
    Runtime::with(|dc| {
        f(dc);
        dc.update();
    })
}
