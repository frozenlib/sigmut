use crate::BindSource;
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
    Runtime::with(|rt| rt.push_bind(task.clone()));
}

pub fn schedule_notify(source: &Rc<impl BindSource>) {
    if source.sinks().set_scheduled() {
        Runtime::with(|rt| rt.push_notify(source.clone()));
    }
}

thread_local! {
    static RUNTIME: RefCell<Option<Runtime>> = RefCell::new(None);
}

struct Runtime {
    depth_notify: usize,
    depth_bind: usize,
    notify_sources: Vec<Rc<dyn BindSource>>,
    bind_tasks: VecDeque<Rc<dyn BindTask>>,
    waker: Option<Waker>,
}
impl Runtime {
    fn new() -> Self {
        spawn_local(TaskRunner::new()).detach();
        Self {
            depth_notify: 0,
            depth_bind: 0,
            notify_sources: Vec::new(),
            bind_tasks: VecDeque::new(),
            waker: None,
        }
    }
    fn with<T>(f: impl FnOnce(&mut Runtime) -> T) -> T {
        RUNTIME.with(|rt| f(rt.borrow_mut().get_or_insert_with(Self::new)))
    }
    fn push_bind(&mut self, task: Rc<dyn BindTask>) {
        self.bind_tasks.push_back(task);
        self.wake();
    }
    fn push_notify(&mut self, source: Rc<dyn BindSource>) {
        self.notify_sources.push(source);
        self.wake();
    }
    fn wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
    fn bind_start(&mut self) {
        if self.depth_notify != 0 {
            panic!("cannot start bind in notify.");
        }
        if self.depth_bind == usize::MAX {
            panic!("bind count overflow.");
        }
        self.depth_bind += 1;
    }
    fn bind_end(&mut self) {
        self.depth_bind -= 1;
    }
    fn notify_start(&mut self) {
        if self.depth_bind != 0 {
            panic!("cannot start notify in bind.");
        }
        if self.depth_notify == usize::MAX {
            panic!("notify count overflow.");
        }
        self.depth_notify += 1;
    }
    fn notify_end(&mut self) {
        self.depth_notify -= 1;
    }

    fn get_task(&mut self, runner: &mut TaskRunner, cx: &mut Context) -> bool {
        swap(&mut self.notify_sources, &mut runner.notify_sources);
        if !runner.notify_sources.is_empty() {
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
    notify_sources: Vec<Rc<dyn BindSource>>,
    bind_task: Option<Rc<dyn BindTask>>,
}

impl TaskRunner {
    fn new() -> Self {
        Self {
            notify_sources: Vec::new(),
            bind_task: None,
        }
    }
}
impl Future for TaskRunner {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        while Runtime::with(|rt| rt.get_task(&mut self, cx)) {
            if !self.notify_sources.is_empty() {
                NotifyScope::with(|scope| {
                    for s in self.notify_sources.drain(..) {
                        s.sinks().notify(scope);
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
