use crate::core::DependencyContext;

pub fn dc_test(f: impl FnOnce(&mut DependencyContext)) {
    DependencyContext::with(|dc| {
        f(dc);
        dc.update();
    })
}
