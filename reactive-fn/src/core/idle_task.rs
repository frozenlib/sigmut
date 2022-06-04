use rt_local_core::{spawn_local, wait_for_idle};
use std::{any::Any, cell::RefCell, mem::swap, rc::Rc, task::Waker};

pub struct Action {
    this: Rc<dyn Any>,
    f: Box<dyn Fn(Rc<dyn Any>)>,
}
impl Action {
    /// Create a new action.
    ///
    /// Should be zero sized type for `f` to avoid heap allocation.
    pub fn new<T, F>(this: Rc<T>, f: F) -> Action
    where
        T: 'static,
        F: Fn(Rc<T>) + 'static,
    {
        Action {
            this,
            f: Box::new(move |this| f(this.downcast::<T>().unwrap())),
        }
    }
    pub fn schedule_normal(self) {
        TaskRunner::with_normal(|runner| runner.push_task(self));
    }
    pub fn schedule_idle(self) {
        TaskRunner::with_idle(|runner| runner.push_task(self));
    }

    fn run(self) {
        (self.f)(self.this)
    }
}

pub trait IdleTask: 'static {
    fn run(self: Rc<Self>);
}
pub fn schedule_idle(task: &Rc<impl IdleTask>) {
    Action::new(task.clone(), IdleTask::run);
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
    fn new(is_idle: bool) -> Self {
        spawn_local(async move {
            let mut tasks = Vec::new();
            loop {
                if is_idle {
                    wait_for_idle().await;
                }
                if is_idle {
                    Self::with_idle(|r| swap(&mut tasks, &mut r.tasks));
                } else {
                    Self::with_normal(|r| swap(&mut tasks, &mut r.tasks));
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

    fn with_normal<T>(f: impl FnOnce(&mut Self) -> T) -> T {
        TASK_RUNNER_NORMAL.with(|r| f(r.borrow_mut().get_or_insert_with(|| TaskRunner::new(false))))
    }
    fn with_idle<T>(f: impl FnOnce(&mut Self) -> T) -> T {
        TASK_RUNNER_IDLE.with(|r| f(r.borrow_mut().get_or_insert_with(|| TaskRunner::new(true))))
    }
    fn push_task(&mut self, task: Action) {
        self.tasks.push(task);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}
