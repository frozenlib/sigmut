use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    panic::{AssertUnwindSafe, catch_unwind},
    pin::Pin,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

use pretty_assertions::assert_eq;
use sigmut::{
    AsyncSignalContext, Reaction, State,
    core::{
        AsyncSourceBinder, BindSink, DirtyLevel, NotifyContext, ReactionContext, Runtime, Slot,
    },
};

struct Noop;
impl BindSink for Noop {
    fn notify(self: Rc<Self>, _: Slot, _: DirtyLevel, _: &mut NotifyContext) {}
}

fn poll_with<T>(
    binder: &mut AsyncSourceBinder,
    mut future: Pin<&mut impl Future<Output = T>>,
    waker: &Waker,
    rc: &mut ReactionContext<'_, '_>,
) -> Poll<T> {
    let mut wrapper = Box::pin(poll_fn(|_| {
        future.as_mut().poll(&mut Context::from_waker(waker))
    }));
    binder.poll(wrapper.as_mut(), rc)
}

#[test]
fn init_rejects_live_context_without_disturbing_pending_work() {
    let mut runtime = Runtime::new();
    let sink = Rc::new(Noop);
    let mut binder = AsyncSourceBinder::new(&Rc::downgrade(&sink));
    let mut context = binder.init(|sc| sc, &mut runtime.rc());
    let mut wait = Box::pin(context.poll_fn(|_| Poll::<()>::Pending));
    assert!(poll_with(&mut binder, wait.as_mut(), Waker::noop(), &mut runtime.rc()).is_pending());
    assert!(binder.is_clean());

    let called = Cell::new(false);
    let result = catch_unwind(AssertUnwindSafe(|| {
        binder.init(
            |sc| {
                called.set(true);
                sc
            },
            &mut runtime.rc(),
        )
    }));
    assert!(result.is_err());
    assert!(!called.get());
    assert!(binder.is_clean());
    assert!(poll_with(&mut binder, wait.as_mut(), Waker::noop(), &mut runtime.rc()).is_pending());

    drop(wait);
    drop(context);
    let _next = binder.init(|sc| sc, &mut runtime.rc());
}

#[test]
fn different_binders_have_independent_contexts() {
    let mut runtime = Runtime::new();
    let sink = Rc::new(Noop);
    let mut first = AsyncSourceBinder::new(&Rc::downgrade(&sink));
    let mut second = AsyncSourceBinder::new(&Rc::downgrade(&sink));
    let _first_context = first.init(|sc| sc, &mut runtime.rc());
    let _second_context = second.init(|sc| sc, &mut runtime.rc());
}

#[test]
fn init_can_retry_after_factory_panic() {
    let mut runtime = Runtime::new();
    let sink = Rc::new(Noop);
    let mut binder = AsyncSourceBinder::new(&Rc::downgrade(&sink));
    let result = catch_unwind(AssertUnwindSafe(|| {
        binder.init::<()>(|_| panic!("factory failed"), &mut runtime.rc())
    }));
    assert!(result.is_err());
    let _next = binder.init(|sc| sc, &mut runtime.rc());
}

#[test]
fn context_retained_after_factory_panic_blocks_init() {
    let mut runtime = Runtime::new();
    let sink = Rc::new(Noop);
    let mut binder = AsyncSourceBinder::new(&Rc::downgrade(&sink));
    let retained = RefCell::new(None);
    let result = catch_unwind(AssertUnwindSafe(|| {
        binder.init::<()>(
            |sc| {
                retained.replace(Some(sc));
                panic!("factory failed after retaining its context");
            },
            &mut runtime.rc(),
        )
    }));
    assert!(result.is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| binder.init(|sc| sc, &mut runtime.rc()))).is_err());
    drop(retained.borrow_mut().take());
    let _next = binder.init(|sc| sc, &mut runtime.rc());
}

#[test]
fn context_owned_by_future_blocks_init_until_future_is_dropped() {
    async fn wait(mut sc: AsyncSignalContext) {
        sc.poll_fn(|_| Poll::<()>::Pending).await;
    }

    let mut runtime = Runtime::new();
    let sink = Rc::new(Noop);
    let mut binder = AsyncSourceBinder::new(&Rc::downgrade(&sink));
    let future = binder.init(wait, &mut runtime.rc());
    assert!(catch_unwind(AssertUnwindSafe(|| binder.init(wait, &mut runtime.rc()))).is_err());
    drop(future);
    let _next = binder.init(wait, &mut runtime.rc());
}

#[rstest::rstest]
fn poll_waker_drop_can_schedule_reaction(#[values(false, true)] ready: bool) {
    struct ScheduleOnDrop(Arc<AtomicUsize>);
    impl Wake for ScheduleOnDrop {
        fn wake(self: Arc<Self>) {
            panic!("polling must not wake this task without a dependency change");
        }
    }
    impl Drop for ScheduleOnDrop {
        fn drop(&mut self) {
            Reaction::new({
                let calls = self.0.clone();
                move |_| {
                    calls.fetch_add(1, Ordering::Relaxed);
                }
            })
            .schedule();
        }
    }

    let mut runtime = Runtime::new();
    let sink = Rc::new(Noop);
    let mut binder = AsyncSourceBinder::new(&Rc::downgrade(&sink));
    let mut context = binder.init(|sc| sc, &mut runtime.rc());
    let complete = Cell::new(false);
    let calls = Arc::new(AtomicUsize::new(0));
    let waker = Waker::from(Arc::new(ScheduleOnDrop(calls.clone())));
    let mut wait = Box::pin(context.poll_fn(|_| {
        if complete.get() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }));
    assert!(poll_with(&mut binder, wait.as_mut(), &waker, &mut runtime.rc()).is_pending());
    drop(waker);
    complete.set(ready);
    assert_eq!(
        poll_with(&mut binder, wait.as_mut(), Waker::noop(), &mut runtime.rc()).is_ready(),
        ready
    );
    runtime.flush();
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
fn poll_fn_uses_latest_waker_and_keeps_ready_dependencies() {
    struct Sink(RefCell<AsyncSourceBinder>);
    impl BindSink for Sink {
        fn notify(self: Rc<Self>, slot: Slot, level: DirtyLevel, _: &mut NotifyContext) {
            self.0.borrow_mut().on_notify(slot, level);
        }
    }
    struct CountWake(AtomicUsize);
    impl Wake for CountWake {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    let mut runtime = Runtime::new();
    let sink = Rc::new_cyclic(|sink| Sink(RefCell::new(AsyncSourceBinder::new(sink))));
    let mut context = sink.0.borrow_mut().init(|sc| sc, &mut runtime.rc());
    let state = State::new(0);
    let old = Arc::new(CountWake(AtomicUsize::new(0)));
    let latest = Arc::new(CountWake(AtomicUsize::new(0)));
    let old_waker = Waker::from(old.clone());
    let latest_waker = Waker::from(latest.clone());
    let mut wait = Box::pin(context.poll_fn(|sc| match state.get(sc) {
        0 => Poll::Pending,
        value => Poll::Ready(value),
    }));
    for waker in [&old_waker, &old_waker, &latest_waker] {
        let mut rc = runtime.rc();
        assert!(poll_with(&mut sink.0.borrow_mut(), wait.as_mut(), waker, &mut rc).is_pending());
    }
    state.set(1, runtime.ac());
    assert_eq!(
        (
            old.0.load(Ordering::Relaxed),
            latest.0.load(Ordering::Relaxed)
        ),
        (0, 1)
    );
    let mut rc = runtime.rc();
    assert_eq!(
        poll_with(
            &mut sink.0.borrow_mut(),
            wait.as_mut(),
            &latest_waker,
            &mut rc
        ),
        Poll::Ready(1)
    );
    assert!(sink.0.borrow().is_clean());
    state.set(2, runtime.ac());
    assert!(!sink.0.borrow().is_clean());
    assert_eq!(
        (
            old.0.load(Ordering::Relaxed),
            latest.0.load(Ordering::Relaxed)
        ),
        (0, 1)
    );
}
