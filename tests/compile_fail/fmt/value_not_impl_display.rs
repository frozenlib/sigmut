use sigmut::{core::Runtime, signal_format};

fn main() {
    let mut rt = Runtime::new();
    struct NoDisplay;
    let _s = signal_format!("{}", NoDisplay);
}
