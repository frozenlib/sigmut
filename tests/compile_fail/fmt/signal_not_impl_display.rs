use sigmut::{core::Runtime, signal_format, Signal};

fn main() {
    let mut rt = Runtime::new();
    struct NoDisplay;
    let s = Signal::from_value(NoDisplay);
    let _s = signal_format!("{}", s);
}
