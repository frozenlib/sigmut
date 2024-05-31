use sigmut::{core::Runtime, signal_format};

fn main() {
    fn f<T>(value: T) {
        let mut rt = Runtime::new();
        let _s = signal_format!("{}", value);
    }
    f(10);
}
