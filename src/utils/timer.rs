use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{Condvar, LazyLock, Mutex},
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};

use futures::future::select;
use futures::{future::Either, pin_mut};
use parse_display::Display;
use slabmap::SlabMap;

pub use sigmut_macros::should_timeout;
pub use sigmut_macros::timeout;

pub mod timeout_helpers;

static SLEEP_REGISTRY: LazyLock<SleepRegistry> = LazyLock::new(|| SleepRegistry {
    queue: Mutex::new(SleepQueue::new()),
    condvar: Condvar::new(),
});

struct SleepRegistry {
    queue: Mutex<SleepQueue>,
    condvar: Condvar,
}
impl SleepRegistry {
    fn run_worker(&self) {
        let mut wakes = Vec::new();
        let mut queue = self.queue.lock().unwrap();
        loop {
            let now = Instant::now();
            let q = &mut *queue;
            while let Some(task) = q.tasks.first_entry()
                && task.key().instant <= now
            {
                wakes.push(q.entries[*task.get()].take().unwrap().waker);
                task.remove();
            }
            if !wakes.is_empty() {
                drop(queue);
                for waker in wakes.drain(..) {
                    waker.wake();
                }
                queue = self.queue.lock().unwrap();
                continue;
            }
            queue = if let Some(task) = queue.tasks.first_key_value() {
                let wait_duration = task.0.instant.saturating_duration_since(now);
                self.condvar.wait_timeout(queue, wait_duration).unwrap().0
            } else {
                self.condvar.wait(queue).unwrap()
            };
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    instant: Instant,
    seq: usize,
}

impl Key {
    fn new(instant: Instant, seq: usize) -> Self {
        Self { instant, seq }
    }
}

struct Entry {
    waker: Waker,
    key: Key,
}
impl Entry {
    fn set_waker(&mut self, waker: &Waker) {
        if !self.waker.will_wake(waker) {
            self.waker = waker.clone();
        }
    }
}

struct SleepQueue {
    next_seqs: BTreeMap<Instant, usize>,
    tasks: BTreeMap<Key, usize>,
    entries: SlabMap<Option<Entry>>,
    thread_running: bool,
}

impl SleepQueue {
    fn lock() -> std::sync::MutexGuard<'static, SleepQueue> {
        SLEEP_REGISTRY.queue.lock().unwrap()
    }

    fn new() -> Self {
        Self {
            next_seqs: BTreeMap::new(),
            tasks: BTreeMap::new(),
            entries: SlabMap::new(),
            thread_running: false,
        }
    }

    fn insert(&mut self, instant: Instant, waker: Waker, condvar: &Condvar) -> usize {
        self.ensure_thread_running();
        let next_seq = self.next_seqs.entry(instant).or_insert(0);
        assert_ne!(
            *next_seq,
            usize::MAX,
            "Too many sleep entries for the same instant"
        );
        let key = Key::new(instant, *next_seq);
        *next_seq += 1;

        let notify = if let Some(first_task) = self.tasks.first_key_value() {
            key < *first_task.0
        } else {
            true
        };
        let id = self.entries.insert(Some(Entry { waker, key }));
        self.tasks.insert(key, id);
        if notify {
            condvar.notify_one();
        }
        id
    }

    fn ensure_thread_running(&mut self) {
        if self.thread_running {
            return;
        }
        self.thread_running = true;
        std::thread::spawn(move || SLEEP_REGISTRY.run_worker());
    }

    fn poll_or_remove(&mut self, id: usize, cx: &Context) -> Poll<()> {
        if let Some(e) = &mut self.entries[id] {
            e.set_waker(cx.waker());
            Poll::Pending
        } else {
            self.entries.remove(id);
            Poll::Ready(())
        }
    }

    fn remove(&mut self, id: usize) {
        if let Some(e) = self.entries.remove(id).unwrap() {
            self.tasks.remove(&e.key);
        }
    }
}

struct WakeAtTask {
    id: Option<usize>,
}

impl WakeAtTask {
    fn schedule(instant: Instant, waker: Waker) -> Self {
        Self {
            id: Some(SleepQueue::lock().insert(instant, waker, &SLEEP_REGISTRY.condvar)),
        }
    }
}

impl Future for WakeAtTask {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(id) = self.id {
            let poll = SleepQueue::lock().poll_or_remove(id, cx);
            if poll.is_ready() {
                self.get_mut().id = None;
            }
            poll
        } else {
            Poll::Ready(())
        }
    }
}
impl Drop for WakeAtTask {
    fn drop(&mut self) {
        if let Some(id) = self.id {
            SleepQueue::lock().remove(id);
        }
    }
}

pub async fn sleep(duration: Duration) {
    if duration > Duration::ZERO {
        WakeAtTask::schedule(Instant::now() + duration, Waker::noop().clone()).await
    }
}
pub async fn sleep_until(instant: Instant) {
    if instant > Instant::now() {
        WakeAtTask::schedule(instant, Waker::noop().clone()).await
    }
}

#[derive(Debug, Display, PartialEq, Eq)]
#[display("timeout")]
pub struct TimeoutError {
    _private: (),
}
impl TimeoutError {
    fn new() -> Self {
        Self { _private: () }
    }
}

impl std::error::Error for TimeoutError {}

pub async fn with_timeout_async<T>(
    fut: impl Future<Output = T>,
    duration: Duration,
) -> Result<T, TimeoutError> {
    let timeout = sleep(duration);
    pin_mut!(fut);
    pin_mut!(timeout);
    match select(fut, timeout).await {
        Either::Left((value, _)) => Ok(value),
        Either::Right((_, _)) => Err(TimeoutError::new()),
    }
}
pub async fn with_timeout_until_async<T>(
    fut: impl Future<Output = T>,
    instant: Instant,
) -> Result<T, TimeoutError> {
    let timeout = sleep_until(instant);
    pin_mut!(fut);
    pin_mut!(timeout);
    match select(fut, timeout).await {
        Either::Left((value, _)) => Ok(value),
        Either::Right((_, _)) => Err(TimeoutError::new()),
    }
}

pub fn with_timeout<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
    duration: Duration,
) -> Result<T, TimeoutError> {
    with_timeout_until(f, Instant::now() + duration)
}

pub fn with_timeout_until<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
    instant: Instant,
) -> Result<T, TimeoutError> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    let timeout = instant.saturating_duration_since(Instant::now());
    rx.recv_timeout(timeout).map_err(|_| TimeoutError::new())
}

#[cfg(test)]
mod tests;
