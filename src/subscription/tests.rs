use super::*;
use assert_call::{CallRecorder, call};
use std::cell::Cell;

fn on_unsubscribe(rc: Rc<Cell<i32>>) {
    call!("{}", rc.get());
}

#[test]
fn from_fn_calls_on_drop() {
    let mut cr = CallRecorder::new();
    {
        let _s = Subscription::from_fn(|| call!("drop"));
    }
    cr.verify("drop");
}

#[test]
fn from_rc_fn_calls_on_drop() {
    let mut cr = CallRecorder::new();
    let rc = Rc::new(Cell::new(7));
    {
        let _s = Subscription::from_rc_fn(rc.clone(), on_unsubscribe);
    }
    cr.verify("7");
}

#[test]
fn from_weak_fn_calls_when_alive() {
    let mut cr = CallRecorder::new();
    let rc = Rc::new(Cell::new(9));
    let weak = Rc::downgrade(&rc);
    {
        let _s = Subscription::from_weak_fn(weak, on_unsubscribe);
    }
    cr.verify("9");
}

#[test]
fn from_weak_fn_noop_when_dead() {
    let mut cr = CallRecorder::new();
    let rc = Rc::new(Cell::new(1));
    let weak = Rc::downgrade(&rc);
    drop(rc);
    {
        let _s = Subscription::from_weak_fn(weak, on_unsubscribe);
    }
    cr.verify(());
}
