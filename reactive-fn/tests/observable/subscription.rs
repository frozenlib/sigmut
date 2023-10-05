use reactive_fn::{core::Runtime, ObsCell, Subscription};

use crate::test_utils::code_path::{code, CodePathChecker};

#[test]
fn new() {
    let mut cp = CodePathChecker::new();
    let mut rt = Runtime::new();
    let sender = ObsCell::new(0);
    let receiver = sender.clone();

    let _s = Subscription::new(move |oc| {
        code(receiver.get(oc));
    });
    rt.update();

    sender.set(1, &mut rt.ac());
    rt.update();

    sender.set(2, &mut rt.ac());
    rt.update();

    cp.expect(["0", "1", "2"]);
    cp.verify();
}

#[test]
fn only_called_in_update() {
    let mut cp = CodePathChecker::new();
    let mut rt = Runtime::new();
    let sender = ObsCell::new(0);
    let receiver = sender.clone();

    let _s = Subscription::new(move |oc| {
        code(receiver.get(oc));
    });

    sender.set(2, &mut rt.ac());
    rt.update();

    sender.set(3, &mut rt.ac());
    sender.set(4, &mut rt.ac());
    rt.update();

    cp.expect(["2", "4"]);
    cp.verify();
}

#[test]
fn new_while() {
    let mut cp = CodePathChecker::new();
    let mut rt = Runtime::new();
    let sender = ObsCell::new(0);
    let receiver = sender.clone();

    let _s = Subscription::new_while(move |oc| {
        let value = receiver.get(oc);
        code(value);
        value < 1
    });
    rt.update();

    sender.set(1, &mut rt.ac());
    rt.update();

    sender.set(2, &mut rt.ac());
    rt.update();

    cp.expect(["0", "1"]);
    cp.verify();
}
