use std::{
    cell::{Cell, RefCell},
    future::Future,
    panic::{AssertUnwindSafe, catch_unwind},
    pin::Pin,
    rc::Rc,
    sync::{Arc, Mutex},
    task::{Context, Poll, Wake, Waker},
};

use futures::Stream;
use pretty_assertions::assert_eq;
use sigmut::{
    AsyncSignalContext, Reaction, Signal, SignalBuilder,
    core::{Runtime, RuntimeCallError},
    effect_async,
};

#[derive(Default)]
struct FutureCalls {
    polls: Cell<usize>,
    drops: Cell<usize>,
    waker: RefCell<Option<Waker>>,
}

struct PanicOnPoll(Rc<FutureCalls>);

impl Future for PanicOnPoll {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        self.0.polls.set(self.0.polls.get() + 1);
        *self.0.waker.borrow_mut() = Some(cx.waker().clone());
        panic!("future poll failed");
    }
}

impl Drop for PanicOnPoll {
    fn drop(&mut self) {
        self.0.drops.set(self.0.drops.get() + 1);
    }
}

#[rstest::rstest]
fn panicking_async_future_is_dropped_without_repolling(#[values(false, true)] effect: bool) {
    let mut runtime = Runtime::new();
    let calls = Rc::new(FutureCalls::default());
    let f = {
        let calls = calls.clone();
        move |_: &mut AsyncSignalContext| PanicOnPoll(calls.clone())
    };
    let (_subscription, signal) = if effect {
        (Some(effect_async(f)), None)
    } else {
        (None, Some(Signal::from_async(f)))
    };
    let drive = |runtime: &mut Runtime| {
        if let Some(signal) = &signal {
            let _ = signal.get(&mut runtime.sc());
        } else {
            runtime.flush();
        }
    };

    assert!(catch_unwind(AssertUnwindSafe(|| drive(&mut runtime))).is_err());
    assert_eq!((calls.polls.get(), calls.drops.get()), (1, 1));
    calls.waker.borrow().as_ref().unwrap().wake_by_ref();
    drive(&mut runtime);
    assert_eq!((calls.polls.get(), calls.drops.get()), (1, 1));
}

struct ReactionOnDrop(Rc<Cell<usize>>);

impl Future for ReactionOnDrop {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        Poll::Ready(())
    }
}

impl Drop for ReactionOnDrop {
    fn drop(&mut self) {
        Reaction::new({
            let calls = self.0.clone();
            move |_| calls.set(calls.get() + 1)
        })
        .schedule();
    }
}

#[rstest::rstest]
fn completed_async_future_can_schedule_reaction(#[values(false, true)] effect: bool) {
    let mut runtime = Runtime::new();
    let calls = Rc::new(Cell::new(0));
    let f = {
        let calls = calls.clone();
        move |_: &mut AsyncSignalContext| ReactionOnDrop(calls.clone())
    };
    let (_subscription, signal) = if effect {
        (Some(effect_async(f)), None)
    } else {
        (None, Some(Signal::from_async(f)))
    };
    if let Some(signal) = signal {
        assert!(signal.get(&mut runtime.sc()).is_ready());
        assert_eq!(calls.get(), 0);
    }
    runtime.flush();
    assert_eq!(calls.get(), 1);
}

#[derive(Clone, Default)]
struct Events(Arc<Mutex<Vec<&'static str>>>);

impl Events {
    fn push(&self, event: &'static str) {
        self.0.lock().unwrap().push(event);
    }
}

struct DropEvent {
    events: Events,
    name: &'static str,
}

impl Drop for DropEvent {
    fn drop(&mut self) {
        self.events.push(self.name);
    }
}

struct ReadyFuture(DropEvent);

impl Future for ReadyFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        self.0.events.push("poll");
        Poll::Ready(())
    }
}

struct EndStream(DropEvent);

impl Stream for EndStream {
    type Item = ();

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<()>> {
        self.0.events.push("poll");
        Poll::Ready(None)
    }
}

struct DriverWake(Events);

impl Wake for DriverWake {
    fn wake(self: Arc<Self>) {
        self.0.push("driver wake");
        assert!(matches!(
            Runtime::try_call(|runtime| runtime.flush()),
            Err(RuntimeCallError::RuntimeUnavailable)
        ));
    }
}

#[rstest::rstest]
fn completed_scan_drops_resources_before_waking_driver(
    #[values(false, true)] stream: bool,
    #[values(false, true)] via_runtime_call: bool,
) {
    let mut runtime = Runtime::new();
    let events = Events::default();
    let signal = if stream {
        SignalBuilder::from_stream_scan_filter(
            0,
            EndStream(DropEvent {
                events: events.clone(),
                name: "stream drop",
            }),
            {
                let capture = DropEvent {
                    events: events.clone(),
                    name: "scan capture drop",
                };
                move |state, item| {
                    assert!(item.is_none());
                    capture.events.push("scan");
                    *state = 42;
                    true
                }
            },
        )
        .build()
    } else {
        SignalBuilder::from_future_scan(
            0,
            ReadyFuture(DropEvent {
                events: events.clone(),
                name: "future drop",
            }),
            {
                let events = events.clone();
                move |state, ()| {
                    events.push("scan");
                    *state = 42;
                }
            },
        )
        .build()
    };
    let waker = Waker::from(Arc::new(DriverWake(events.clone())));
    let mut lend = runtime.lend();
    {
        let mut wait = Box::pin(lend.wait_for_ready());
        assert!(
            wait.as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_pending()
        );
    }
    if via_runtime_call {
        Runtime::call(|runtime| assert_eq!(signal.get(&mut runtime.sc()), 42));
        drop(lend);
    } else {
        drop(lend);
        assert_eq!(signal.get(&mut runtime.sc()), 42);
    }
    assert_eq!(signal.get(&mut runtime.sc()), 42);
    assert_eq!(
        *events.0.lock().unwrap(),
        if stream {
            vec![
                "poll",
                "scan",
                "stream drop",
                "scan capture drop",
                "driver wake",
            ]
        } else {
            vec!["poll", "scan", "future drop", "driver wake"]
        }
    );
}
