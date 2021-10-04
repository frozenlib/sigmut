use std::{cell::RefCell, cell::RefMut, collections::VecDeque, rc::Rc};

pub struct Runtime(RefCell<RuntimeData>);

struct RuntimeData {
    state: RuntimeState,
    tasks_notify: VecDeque<Rc<dyn NotifyTask>>,
    tasks_bind_normal: VecDeque<Rc<dyn BindTask>>,
    tasks_bind_idle: VecDeque<Rc<dyn BindTask>>,
}

impl Runtime {
    fn new() -> Self {
        Self(RefCell::new(RuntimeData {
            state: RuntimeState::None,
            tasks_notify: VecDeque::new(),
            tasks_bind_normal: VecDeque::new(),
            tasks_bind_idle: VecDeque::new(),
        }))
    }
    fn notify_inline<T>(&self, f: impl FnOnce(&NotifyScope) -> T) -> T {
        let d = self.0.borrow_mut();
        match d.state {
            RuntimeState::None => self.notify_start(d, f),
            RuntimeState::Notify => {
                drop(d);
                f(self.as_notify_scope())
            }
            RuntimeState::Bind => {
                panic!("while `BindScope` exists, `NotifyScope` cannot be created.")
            }
        }
    }
    fn notify_start<T>(&self, mut d: RefMut<RuntimeData>, f: impl FnOnce(&NotifyScope) -> T) -> T {
        d.state = RuntimeState::Notify;
        drop(d);
        let retval = f(self.as_notify_scope());
        self.run_tasks();
        retval
    }
    fn notify_defer(&self, task: Rc<dyn NotifyTask>) {
        let mut d = self.0.borrow_mut();
        match d.state {
            RuntimeState::Notify | RuntimeState::Bind => d.tasks_notify.push_back(task),
            RuntimeState::None => panic!("called `notify_defer` while task was not running."),
        }
    }
    fn notify_schedule(&self, task: Rc<dyn NotifyTask>) {
        let mut d = self.0.borrow_mut();
        match d.state {
            RuntimeState::None => self.notify_start(d, |scope| task.run(scope)),
            RuntimeState::Notify => task.run(self.as_notify_scope()),
            RuntimeState::Bind => d.tasks_notify.push_back(task),
        }
    }
    fn bind_inline<T>(&self, f: impl FnOnce(&BindScope) -> T) -> T {
        let d = self.0.borrow_mut();
        match d.state {
            RuntimeState::None => self.bind_start(d, f),
            RuntimeState::Bind => {
                drop(d);
                f(self.as_bind_scope())
            }
            RuntimeState::Notify => {
                panic!("while `NotifyScope` exists, `BindScope` cannot be created.")
            }
        }
    }
    fn bind_start<T>(&self, mut d: RefMut<RuntimeData>, f: impl FnOnce(&BindScope) -> T) -> T {
        d.state = RuntimeState::Bind;
        drop(d);
        let retval = f(self.as_bind_scope());
        self.run_tasks();
        retval
    }
    fn bind_defer(&self, task: Rc<dyn BindTask>, priority: TaskPriority) {
        let mut d = self.0.borrow_mut();
        match d.state {
            RuntimeState::None => panic!("called `notify_defer` while task was not running."),
            RuntimeState::Notify | RuntimeState::Bind => d.tasks_bind(priority).push_back(task),
        }
    }
    fn bind_schedule(&self, task: Rc<dyn BindTask>, priority: TaskPriority) {
        let mut d = self.0.borrow_mut();
        match d.state {
            RuntimeState::None => self.bind_start(d, |scope| task.run(scope)),
            RuntimeState::Notify => task.run(self.as_bind_scope()),
            RuntimeState::Bind => d.tasks_bind(priority).push_back(task),
        }
    }

    fn as_notify_scope(&self) -> &NotifyScope {
        unsafe { &*(self as *const Self as *const NotifyScope) }
    }
    fn as_bind_scope(&self) -> &BindScope {
        unsafe { &*(self as *const Self as *const BindScope) }
    }

    fn run_tasks(&self) {
        loop {
            let mut d = self.0.borrow_mut();
            if let Some(task) = d.tasks_notify.pop_front() {
                d.state = RuntimeState::Notify;
                drop(d);
                task.run(self.as_notify_scope());
                continue;
            }
            if let Some(task) = d.pop_bind_task() {
                d.state = RuntimeState::Bind;
                drop(d);
                task.run(self.as_bind_scope());
                continue;
            }
            d.state = RuntimeState::None;
            break;
        }
    }
    fn with<T>(f: impl FnOnce(&Self) -> T) -> T {
        thread_local!(static RT: Runtime = Runtime::new());
        RT.with(|rt| f(rt))
    }
}
impl RuntimeData {
    fn tasks_bind(&mut self, priority: TaskPriority) -> &mut VecDeque<Rc<dyn BindTask>> {
        match priority {
            TaskPriority::Normal => &mut self.tasks_bind_normal,
            TaskPriority::Idle => &mut self.tasks_bind_idle,
        }
    }
    fn pop_bind_task(&mut self) -> Option<Rc<dyn BindTask>> {
        self.tasks_bind_normal
            .pop_back()
            .or_else(|| self.tasks_bind_idle.pop_back())
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum TaskPriority {
    Normal,
    Idle,
}

enum RuntimeState {
    None,
    Notify,
    Bind,
}

#[repr(transparent)]
pub struct BindScope(Runtime);

impl BindScope {
    pub fn with<T>(f: impl FnOnce(&BindScope) -> T) -> T {
        Runtime::with(|rt| rt.bind_inline(f))
    }
    pub fn defer_notify(&self, task: Rc<dyn NotifyTask>) {
        self.0.notify_defer(task)
    }
}

#[repr(transparent)]
pub struct NotifyScope(Runtime);

impl NotifyScope {
    pub fn with<T>(f: impl FnOnce(&NotifyScope) -> T) -> T {
        Runtime::with(|rt| rt.notify_inline(f))
    }
    pub fn defer_bind(&self, task: Rc<dyn BindTask>) {
        self.0.bind_defer(task, TaskPriority::Normal)
    }
}

pub trait BindTask: 'static {
    fn run(self: Rc<Self>, scope: &BindScope);
    fn schedule(self: &Rc<Self>)
    where
        Self: Sized,
    {
        let task = self.clone();
        Runtime::with(|rt| rt.bind_schedule(task, TaskPriority::Normal))
    }
}

pub trait NotifyTask: 'static {
    fn run(self: Rc<Self>, scope: &NotifyScope);
    fn schedule(self: &Rc<Self>)
    where
        Self: Sized,
    {
        let task = self.clone();
        Runtime::with(|rt| rt.notify_schedule(task))
    }
}
