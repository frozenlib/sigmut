use futures_core::Future;
use rt_local::{spawn_local, yield_now, Task};
use std::{
    cell::RefCell,
    mem::swap,
    pin::Pin,
    rc::{Rc, Weak},
    task::{Context, Poll, Waker},
};

pub fn spawn_local_weak<F: RcFuture<Output = ()>>(f: &Rc<F>) -> Task<()> {
    let f: Rc<dyn RcFuture<Output = ()>> = f.clone();
    spawn_local(WeakRcFuture(Rc::downgrade(&f)))
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
    _task: Task<()>,
}

impl IdleTaskRunner {
    fn new() -> Self {
        Self {
            tasks: Vec::new(),
            waker: None,
            _task: spawn_local(async {
                let mut tasks = Vec::new();
                loop {
                    yield_now().await;
                    Self::with(|r| swap(&mut tasks, &mut r.tasks));
                    for task in tasks.drain(..) {
                        task.call();
                    }
                }
            }),
        }
    }

    fn with<T>(f: impl FnOnce(&mut Self) -> T) -> T {
        IDLE_TASK_RUNNER.with(|r| {
            let mut r = r.borrow_mut();
            loop {
                if let Some(r) = r.as_mut() {
                    return f(r);
                } else {
                    *r = Some(IdleTaskRunner::new());
                }
            }
        })
    }
    fn push_task(&mut self, task: Rc<dyn IdleTask>) {
        self.tasks.push(task);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}
