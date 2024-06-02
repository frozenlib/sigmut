use std::any::Any;

use assert_call::call;

pub fn call_on_drop(s: &'static str) -> impl Any {
    struct OnDrop(&'static str);
    impl Drop for OnDrop {
        fn drop(&mut self) {
            call!("{}", self.0);
        }
    }
    OnDrop(s)
}
