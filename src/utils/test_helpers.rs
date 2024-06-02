use std::any::Any;

use assert_call::call;

pub fn call_on_drop(s: impl std::fmt::Display) -> impl Any {
    struct OnDrop(String);
    impl Drop for OnDrop {
        fn drop(&mut self) {
            call!("{}", self.0);
        }
    }
    OnDrop(s.to_string())
}
