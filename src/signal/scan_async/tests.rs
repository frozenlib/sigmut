use super::*;
use crate::{State, core::Runtime};
use assert_call::{CallRecorder, call};
use pretty_assertions::assert_eq;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    task::{Context, Waker},
};

#[test]
fn replacement_and_completion_keep_scan_before_drop() {
    struct RecordedFuture {
        _context: AsyncSignalContext,
        value: i32,
    }
    impl Future for RecordedFuture {
        type Output = i32;

        fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<i32> {
            call!("poll {}", self.value);
            if self.value == 1 {
                Poll::Pending
            } else {
                Poll::Ready(self.value)
            }
        }
    }
    impl Drop for RecordedFuture {
        fn drop(&mut self) {
            call!("drop {}", self.value);
        }
    }
    let mut runtime = Runtime::new();
    let mut calls = CallRecorder::new();
    let dependency = State::new(1);
    let signal = build_scan_async(
        0,
        {
            let dependency = dependency.clone();
            move |mut context| {
                let value = context.with(|sc| dependency.get(sc));
                call!("init {value}");
                RecordedFuture {
                    _context: context,
                    value,
                }
            }
        },
        |state, value| {
            call!("scan {value:?}");
            if let Poll::Ready(value) = value {
                *state = value;
            }
            true
        },
        |state| state,
    );

    assert_eq!(signal.get(&mut runtime.sc()), 0);
    calls.verify(["init 1", "poll 1", "scan Pending"]);
    dependency.set(2, runtime.ac());
    assert_eq!(signal.get(&mut runtime.sc()), 2);
    calls.verify(["drop 1", "init 2", "poll 2", "scan Ready(2)", "drop 2"]);
}

#[rstest::rstest]
fn scan_panic_retains_only_pending_future(#[values(false, true)] ready: bool) {
    struct CountedFuture {
        _context: AsyncSignalContext,
        polls: Rc<Cell<usize>>,
        drops: Rc<Cell<usize>>,
        waker: Rc<RefCell<Option<Waker>>>,
        ready: bool,
    }
    impl Future for CountedFuture {
        type Output = ();

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            self.polls.set(self.polls.get() + 1);
            *self.waker.borrow_mut() = Some(cx.waker().clone());
            if self.ready {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        }
    }
    impl Drop for CountedFuture {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }
    let mut runtime = Runtime::new();
    let polls = Rc::new(Cell::new(0));
    let drops = Rc::new(Cell::new(0));
    let waker = Rc::new(RefCell::new(None));
    let signal = build_scan_async(
        (),
        {
            let polls = polls.clone();
            let drops = drops.clone();
            let waker = waker.clone();
            move |context| CountedFuture {
                _context: context,
                polls: polls.clone(),
                drops: drops.clone(),
                waker: waker.clone(),
                ready,
            }
        },
        |_, _| panic!("scan failed"),
        |state| state,
    );
    assert!(catch_unwind(AssertUnwindSafe(|| signal.get(&mut runtime.sc()))).is_err());
    assert_eq!((polls.get(), drops.get()), (1, usize::from(ready)));

    waker.borrow().as_ref().unwrap().wake_by_ref();
    signal.get(&mut runtime.sc());
    assert_eq!(
        (polls.get(), drops.get()),
        if ready { (1, 1) } else { (2, 0) }
    );
}
