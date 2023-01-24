use reactive_fn::core::DependencyContext;

pub mod code_path;

pub fn dc_test(f: impl FnOnce(&mut DependencyContext)) {
    DependencyContext::with(|dc| {
        f(dc);
        dc.update();
    })
}
