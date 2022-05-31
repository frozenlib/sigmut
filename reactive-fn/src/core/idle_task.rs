use rt_local_core::{spawn_local, wait_for_idle};
use std::{cell::RefCell, mem::swap, rc::Rc, task::Waker};

pub trait IdleTask: 'static {
    fn run(self: Rc<Self>);
}
pub fn schedule_idle(task: &Rc<impl IdleTask>) {
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
                wait_for_idle().await;
                Self::with(|r| swap(&mut tasks, &mut r.tasks));
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
