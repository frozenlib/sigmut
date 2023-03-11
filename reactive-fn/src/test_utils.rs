use crate::core::Runtime;

pub fn dc_test(f: impl FnOnce(&mut Runtime)) {
    Runtime::with(|dc| {
        f(dc);
        dc.update();
    })
}
