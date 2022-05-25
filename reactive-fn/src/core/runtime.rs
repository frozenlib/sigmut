use rt_local_core::spawn_local;
use std::{
    cell::RefCell,
    collections::VecDeque,
    future::Future,
    mem::swap,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

pub struct BindScope {
    _dummy: (),
}

impl BindScope {
    pub fn with<T>(f: impl FnOnce(&BindScope) -> T) -> T {
        Runtime::with(|rt| rt.bind_start());
        let value = f(&BindScope { _dummy: () });
        Runtime::with(|rt| rt.bind_end());
        value
    }
}

pub struct NotifyScope {
    _dummy: (),
}

impl NotifyScope {
    pub fn with<T>(f: impl FnOnce(&NotifyScope) -> T) -> T {
        Runtime::with(|rt| rt.notify_start());
        let value = f(&NotifyScope { _dummy: () });
        Runtime::with(|rt| rt.notify_end());
        value
    }
}

pub trait BindTask: 'static {
    fn run(self: Rc<Self>, scope: &BindScope);
}
pub fn schedule_bind(task: &Rc<impl BindTask>) {
    Runtime::with(|rt| rt.push_bind_task(task.clone()));
}

pub trait NotifyTask: 'static {
    fn run(self: Rc<Self>, scope: &NotifyScope);
}
pub fn schedule_notify(task: &Rc<impl NotifyTask>) {
    Runtime::with(|rt| rt.push_notify_task(task.clone()));
}

thread_local! {
    static RUNTIME: RefCell<Option<Runtime>> = RefCell::new(None);
}

struct Runtime {
    notify_count: usize,
    notify_tasks: Vec<Rc<dyn NotifyTask>>,
    bind_count: usize,
    bind_tasks: VecDeque<Rc<dyn BindTask>>,
    waker: Option<Waker>,
}
impl Runtime {
    fn new() -> Self {
        spawn_local(TaskRunner::new()).detach();
        Self {
            notify_count: 0,
            notify_tasks: Vec::new(),
            bind_count: 0,
            bind_tasks: VecDeque::new(),
            waker: None,
        }
    }
    fn with<T>(f: impl FnOnce(&mut Runtime) -> T) -> T {
        RUNTIME.with(|rt| f(rt.borrow_mut().get_or_insert_with(Self::new)))
    }
    fn push_bind_task(&mut self, task: Rc<dyn BindTask>) {
        self.bind_tasks.push_back(task);
        self.wake();
    }
    fn push_notify_task(&mut self, task: Rc<dyn NotifyTask>) {
        self.notify_tasks.push(task);
        self.wake();
    }
    fn wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
    fn bind_start(&mut self) {
        if self.notify_count != 0 {
            panic!("cannot start bind in notify.");
        }
        if self.bind_count == usize::MAX {
            panic!("bind count overflow.");
        }
        self.bind_count += 1;
    }
    fn bind_end(&mut self) {
        self.bind_count -= 1;
    }
    fn notify_start(&mut self) {
        if self.bind_count != 0 {
            panic!("cannot start notify in bind.");
        }
        if self.notify_count == usize::MAX {
            panic!("notify count overflow.");
        }
        self.notify_count += 1;
    }
    fn notify_end(&mut self) {
        self.notify_count -= 1;
    }

    fn get_tasks(&mut self, runner: &mut TaskRunner, cx: &mut Context) -> bool {
        swap(&mut self.notify_tasks, &mut runner.notify_tasks);
        if !runner.notify_tasks.is_empty() {
            return true;
        }
        runner.bind_task = self.bind_tasks.pop_front();
        if runner.bind_task.is_some() {
            return true;
        }
        self.waker = Some(cx.waker().clone());
        false
    }
}
struct TaskRunner {
    notify_tasks: Vec<Rc<dyn NotifyTask>>,
    bind_task: Option<Rc<dyn BindTask>>,
}

impl TaskRunner {
    fn new() -> Self {
        Self {
            notify_tasks: Vec::new(),
            bind_task: None,
        }
    }
}
impl Future for TaskRunner {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        while Runtime::with(|rt| rt.get_tasks(&mut self, cx)) {
            if !self.notify_tasks.is_empty() {
                NotifyScope::with(|scope| {
                    for task in self.notify_tasks.drain(..) {
                        task.run(scope);
                    }
                });
            }
            if let Some(task) = self.bind_task.take() {
                BindScope::with(|scope| task.run(scope));
            }
        }
        Poll::Pending
    }
}
