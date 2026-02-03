use std::{
    future::Future,
    pin::{Pin, pin},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
    thread,
    time::{Duration, Instant},
};

use pretty_assertions::assert_eq;

use super::*;

struct CountingWake {
    count: AtomicUsize,
}

impl CountingWake {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            count: AtomicUsize::new(0),
        })
    }

    fn count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    fn waker(self: &Arc<Self>) -> Waker {
        Waker::from(self.clone())
    }
}

impl Wake for CountingWake {
    fn wake(self: Arc<Self>) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

struct OrderWake {
    id: u8,
    log: Arc<Mutex<Vec<u8>>>,
}

impl Wake for OrderWake {
    fn wake(self: Arc<Self>) {
        self.log.lock().unwrap().push(self.id);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.log.lock().unwrap().push(self.id);
    }
}

fn order_waker(id: u8, log: Arc<Mutex<Vec<u8>>>) -> Waker {
    Waker::from(Arc::new(OrderWake { id, log }))
}

fn poll_future(fut: Pin<&mut impl Future<Output = ()>>, waker: &Waker) -> Poll<()> {
    let mut cx = Context::from_waker(waker);
    fut.poll(&mut cx)
}

fn wait_until(timeout: Duration, mut f: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if f() {
            return true;
        }
        thread::sleep(Duration::from_millis(1));
    }
    f()
}

#[test]
fn sleep_completes_after_duration() {
    let start = Instant::now();
    futures::executor::block_on(sleep(Duration::from_millis(20)));
    assert!(start.elapsed() >= Duration::from_millis(15));
}

#[test]
fn sleep_until_past_is_ready_immediately() {
    let mut fut = pin!(sleep_until(Instant::now() - Duration::from_millis(1)));
    let waker = Waker::noop();
    assert!(matches!(poll_future(fut.as_mut(), waker), Poll::Ready(())));
}

#[test]
fn updates_waker_when_polled_again() {
    let mut fut = pin!(sleep(Duration::from_millis(30)));
    let first = CountingWake::new();
    let second = CountingWake::new();

    assert!(matches!(
        poll_future(fut.as_mut(), &first.waker()),
        Poll::Pending
    ));
    assert!(matches!(
        poll_future(fut.as_mut(), &second.waker()),
        Poll::Pending
    ));

    assert!(wait_until(Duration::from_millis(200), || second.count() >= 1));
    assert_eq!(first.count(), 0);
}

#[test]
fn earlier_sleep_wakes_first_even_if_registered_later() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let waker_late = order_waker(1, log.clone());
    let waker_early = order_waker(2, log.clone());

    let mut late = pin!(sleep(Duration::from_millis(60)));
    let mut early = pin!(sleep(Duration::from_millis(20)));

    assert!(matches!(
        poll_future(late.as_mut(), &waker_late),
        Poll::Pending
    ));
    assert!(matches!(
        poll_future(early.as_mut(), &waker_early),
        Poll::Pending
    ));

    assert!(wait_until(Duration::from_millis(200), || {
        log.lock().unwrap().len() >= 2
    }));
    assert_eq!(&*log.lock().unwrap(), &[2, 1]);
}

#[test]
fn drop_removes_sleep_from_queue() {
    let mut fut = Box::pin(sleep(Duration::from_millis(30)));
    let waker = CountingWake::new();
    assert!(matches!(
        poll_future(fut.as_mut(), &waker.waker()),
        Poll::Pending
    ));
    drop(fut);

    assert!(!wait_until(Duration::from_millis(120), || waker.count() > 0));
    assert_eq!(waker.count(), 0);
}

#[test]
fn multiple_sleeps_wake_when_time_is_reached() {
    let when = Instant::now() + Duration::from_millis(25);
    let mut first_fut = pin!(sleep_until(when));
    let mut second_fut = pin!(sleep_until(when));
    let first = CountingWake::new();
    let second = CountingWake::new();

    assert!(matches!(
        poll_future(first_fut.as_mut(), &first.waker()),
        Poll::Pending
    ));
    assert!(matches!(
        poll_future(second_fut.as_mut(), &second.waker()),
        Poll::Pending
    ));

    assert!(wait_until(Duration::from_millis(200), || {
        first.count() >= 1 && second.count() >= 1
    }));
    assert_eq!(first.count(), 1);
    assert_eq!(second.count(), 1);
}

#[test]
fn promotes_new_wakers_when_earliest_is_removed() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let first = CountingWake::new();
    let waker_second = order_waker(1, log.clone());
    let waker_third = order_waker(2, log.clone());

    let mut earliest = Box::pin(sleep(Duration::from_millis(20)));
    let mut second = pin!(sleep(Duration::from_millis(60)));
    let mut third = pin!(sleep(Duration::from_millis(80)));

    assert!(matches!(
        poll_future(earliest.as_mut(), &first.waker()),
        Poll::Pending
    ));
    assert!(matches!(
        poll_future(second.as_mut(), &waker_second),
        Poll::Pending
    ));
    assert!(matches!(
        poll_future(third.as_mut(), &waker_third),
        Poll::Pending
    ));

    drop(earliest);

    assert!(wait_until(Duration::from_millis(200), || {
        log.lock().unwrap().len() >= 2
    }));
    assert_eq!(&*log.lock().unwrap(), &[1, 2]);
    assert_eq!(first.count(), 0);
}

#[test]
fn concurrent_sleep_until_does_not_deadlock() {
    let base = Instant::now() + Duration::from_millis(30);
    let handles: Vec<_> = (0..4)
        .map(|i| {
            thread::spawn(move || {
                futures::executor::block_on(sleep_until(base + Duration::from_millis(i * 5)));
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn with_timeout_completes_before_deadline() {
    let result = with_timeout(|| 42, Duration::from_millis(100));
    assert_eq!(result, Ok(42));
}

#[test]
fn with_timeout_times_out() {
    let result = with_timeout(
        || {
            thread::sleep(Duration::from_millis(200));
            42
        },
        Duration::from_millis(50),
    );
    assert!(result.is_err());
}
