use assert_call::{call, CallRecorder};
use reactive_fn::{core::Runtime, ObsCell, Subscription};

#[test]
fn new() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let sender = ObsCell::new(0);
    let receiver = sender.clone();

    let _s = Subscription::new(move |oc| {
        call!("{}", receiver.get(oc));
    });
    rt.update();

    sender.set(1, &mut rt.ac());
    rt.update();

    sender.set(2, &mut rt.ac());
    rt.update();

    c.verify(["0", "1", "2"]);
}

#[test]
fn only_called_in_update() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let sender = ObsCell::new(0);
    let receiver = sender.clone();

    let _s = Subscription::new(move |oc| {
        call!("{}", receiver.get(oc));
    });

    sender.set(2, &mut rt.ac());
    rt.update();

    sender.set(3, &mut rt.ac());
    sender.set(4, &mut rt.ac());
    rt.update();

    c.verify(["2", "4"]);
}

#[test]
fn new_while() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let sender = ObsCell::new(0);
    let receiver = sender.clone();

    let _s = Subscription::new_while(move |oc| {
        let value = receiver.get(oc);
        call!("{value}");
        value < 1
    });
    rt.update();

    sender.set(1, &mut rt.ac());
    rt.update();

    sender.set(2, &mut rt.ac());
    rt.update();

    c.verify(["0", "1"]);
}
