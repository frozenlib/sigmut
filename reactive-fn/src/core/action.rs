use rt_local_core::{spawn_local, wait_for_idle};
use std::{
    any::Any,
    cell::RefCell,
    future::Future,
    mem::swap,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

pub struct Action {
    s: Rc<dyn Any>,
    f: Box<dyn Fn(Rc<dyn Any>)>,
}
impl Action {
    /// Create a new action.
    ///
    /// Should be zero sized type for `f` to avoid heap allocation.
    pub fn new<T, F>(s: Rc<T>, f: F) -> Action
    where
        T: 'static,
        F: Fn(Rc<T>) + 'static,
    {
        Action {
            s,
            f: Box::new(move |this| f(this.downcast::<T>().unwrap())),
        }
    }
    pub fn schedule_idle(self) {
        self.schedule(ActionPriority::Idle)
    }
    pub fn schedule_normal(self) {
        self.schedule(ActionPriority::Normal)
    }
    fn schedule(self, priority: ActionPriority) {
        TaskRunner::with(priority, |runner| runner.push_task(self));
    }

    fn run(self) {
        (self.f)(self.s)
    }
}
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum ActionPriority {
    Idle,
    Normal,
}

thread_local! {
    static TASK_RUNNER_IDLE: RefCell<Option<TaskRunner>> = RefCell::new(None);
    static TASK_RUNNER_NORMAL: RefCell<Option<TaskRunner>> = RefCell::new(None);
}

struct TaskRunner {
    tasks: Vec<Action>,
    waker: Option<Waker>,
}

impl TaskRunner {
    fn new(priority: ActionPriority) -> Self {
        spawn_local(async move {
            let mut tasks = Vec::new();
            loop {
                if priority == ActionPriority::Idle {
                    wait_for_idle().await;
                }
                Self::with(priority, |r| swap(&mut tasks, &mut r.tasks));
                if tasks.is_empty() {
                    SetWaker(Some(|waker| {
                        Self::with(priority, |this| this.waker = Some(waker))
                    }))
                    .await;
                    continue;
                }
                for task in tasks.drain(..) {
                    task.run();
                }
            }
        })
        .detach();
        Self {
            tasks: Vec::new(),
            waker: None,
        }
    }

    fn with<T>(priority: ActionPriority, f: impl FnOnce(&mut Self) -> T) -> T {
        let key = match priority {
            ActionPriority::Idle => &TASK_RUNNER_IDLE,
            ActionPriority::Normal => &TASK_RUNNER_NORMAL,
        };
        key.with(|r| {
            f(r.borrow_mut()
                .get_or_insert_with(|| TaskRunner::new(priority)))
        })
    }
    fn push_task(&mut self, task: Action) {
        self.tasks.push(task);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

struct SetWaker<F>(Option<F>);

impl<F: FnOnce(Waker) + Unpin> Future for SetWaker<F> {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(f) = self.0.take() {
            f(cx.waker().clone());
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}
