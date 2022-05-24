use futures_core::Future;
use rt_local_core::{spawn_local, yield_now, Task};
use std::{
    cell::RefCell,
    mem::swap,
    pin::Pin,
    rc::{Rc, Weak},
    task::{Context, Poll, Waker},
};

pub fn spawn_local_weak<F: RcFuture<Output = ()>>(f: &Rc<F>) -> Task<()> {
    spawn_local_weak_raw(Rc::downgrade(f))
}
pub fn spawn_local_weak_raw<F: RcFuture<Output = ()>>(f: Weak<F>) -> Task<()> {
    let f: Weak<dyn RcFuture<Output = ()>> = f;
    spawn_local(WeakRcFuture(f))
}

pub trait RcFuture: 'static {
    type Output;
    fn poll(self: Rc<Self>, cx: &mut Context) -> Poll<Self::Output>;
}
struct WeakRcFuture(Weak<dyn RcFuture<Output = ()>>);

impl Future for WeakRcFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(f) = self.get_mut().0.upgrade() {
            f.poll(cx)
        } else {
            Poll::Ready(())
        }
    }
}

pub trait IdleTask: 'static {
    fn call(self: Rc<Self>);
}
pub fn call_on_idle(task: &Rc<impl IdleTask>) {
    IdleTaskRunner::with(|r| r.push_task(task.clone()));
}

thread_local! {
    static IDLE_TASK_RUNNER: RefCell<Option<IdleTaskRunner>> = RefCell::new(None);
}

struct IdleTaskRunner {
    tasks: Vec<Rc<dyn IdleTask>>,
    waker: Option<Waker>,
}

impl IdleTaskRunner {
    fn new() -> Self {
        spawn_local(async {
            let mut tasks = Vec::new();
            loop {
                yield_now().await;
                Self::with(|r| swap(&mut tasks, &mut r.tasks));
                for task in tasks.drain(..) {
                    task.call();
                }
            }
        })
        .detach();
        Self {
            tasks: Vec::new(),
            waker: None,
        }
    }

    fn with<T>(f: impl FnOnce(&mut Self) -> T) -> T {
        IDLE_TASK_RUNNER.with(|r| f(r.borrow_mut().get_or_insert_with(IdleTaskRunner::new)))
    }
    fn push_task(&mut self, task: Rc<dyn IdleTask>) {
        self.tasks.push(task);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}
