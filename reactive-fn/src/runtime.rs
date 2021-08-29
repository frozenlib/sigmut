use std::{cell::RefCell, cell::RefMut, mem::swap, rc::Rc};

pub struct Runtime(BindScope);
struct RuntimeData {
    state: RuntimeState,
    bind_tasks: Vec<Rc<dyn BindTask>>,
    notify_tasks: Vec<Rc<dyn NotifyTask>>,
}
enum RuntimeState {
    None,
    Notify,
    Bind,
}
pub struct BindScope(NotifyScope);

impl BindScope {
    pub fn with<T>(f: impl FnOnce(&BindScope) -> T) -> T {
        Runtime::with(|this| {
            this.try_bind(
                (),
                |_, bc| f(bc),
                |_, _| panic!("Cannot create BindScope when NotifyScope exists."),
            )
        })
    }
    pub fn defer_notify(&self, task: Rc<dyn NotifyTask>) {
        (self.0).0.borrow_mut().notify_tasks.push(task);
    }
}

pub struct NotifyScope(RefCell<RuntimeData>);

impl NotifyScope {
    pub fn with<T>(f: impl FnOnce(&NotifyScope) -> T) -> T {
        Runtime::with(|this| {
            this.try_notify(
                (),
                |_, bc| f(bc),
                |_, _| panic!("Cannot create NotifyScope when BindScope exists."),
            )
        })
    }
    pub fn defer_bind(&self, task: Rc<dyn BindTask>) {
        self.0.borrow_mut().bind_tasks.push(task);
    }
}

pub trait BindTask: 'static {
    fn run(self: Rc<Self>, scope: &BindScope);
}

pub trait NotifyTask: 'static {
    fn run(self: Rc<Self>, scope: &NotifyScope);
}

impl Runtime {
    fn new() -> Self {
        Self(BindScope(NotifyScope(RefCell::new(RuntimeData {
            state: RuntimeState::None,
            bind_tasks: Vec::new(),
            notify_tasks: Vec::new(),
        }))))
    }
    pub fn spawn_bind(task: Rc<dyn BindTask>) {
        Runtime::with(|rt| {
            rt.try_bind(
                task,
                |task, scope| task.run(scope),
                |task, rt| rt.bind_tasks.push(task),
            )
        })
    }
    pub fn spawn_notify(task: Rc<dyn NotifyTask>) {
        Runtime::with(|rt| {
            rt.try_notify(
                task,
                |task, scope| task.run(scope),
                |task, rt| rt.notify_tasks.push(task),
            )
        })
    }

    fn try_notify<T, A>(
        &self,
        arg: A,
        on_ok: impl FnOnce(A, &NotifyScope) -> T,
        on_err: impl FnOnce(A, &mut RuntimeData) -> T,
    ) -> T {
        let value;
        let mut b = self.borrow_mut();
        match b.state {
            RuntimeState::None => {
                b.state = RuntimeState::Notify;
                drop(b);
                value = on_ok(arg, self.notify_scope());
                self.notify_end(self.borrow_mut());
            }
            RuntimeState::Notify => {
                drop(b);
                value = on_ok(arg, self.notify_scope());
            }
            RuntimeState::Bind => {
                value = on_err(arg, &mut b);
            }
        }
        value
    }
    fn notify_end(&self, b: RefMut<RuntimeData>) {
        let mut b = b;
        if b.bind_tasks.is_empty() {
            b.state = RuntimeState::None;
            return;
        }
        b.state = RuntimeState::Bind;
        while let Some(task) = b.bind_tasks.pop() {
            drop(b);
            task.run(self.bind_scope());
            b = self.borrow_mut();
        }
        self.bind_end(b);
    }
    fn try_bind<T, A>(
        &self,
        arg: A,
        on_ok: impl FnOnce(A, &BindScope) -> T,
        on_err: impl FnOnce(A, &mut RuntimeData) -> T,
    ) -> T {
        let mut b = self.borrow_mut();
        let value;
        match b.state {
            RuntimeState::None => {
                b.state = RuntimeState::Bind;
                drop(b);
                value = on_ok(arg, self.bind_scope());
                self.bind_end(self.borrow_mut());
            }
            RuntimeState::Bind => {
                drop(b);
                value = on_ok(arg, self.bind_scope());
            }
            RuntimeState::Notify => {
                value = on_err(arg, &mut b);
            }
        }
        value
    }

    fn bind_end(&self, b: RefMut<RuntimeData>) {
        let mut b = b;
        b.state = RuntimeState::None;
        if b.notify_tasks.is_empty() {
            return;
        }
        b.state = RuntimeState::Notify;
        let mut tasks = Vec::new();
        swap(&mut b.notify_tasks, &mut tasks);
        drop(b);
        for task in tasks.drain(..) {
            task.run(self.notify_scope());
        }
        b = self.borrow_mut();
        b.notify_tasks = tasks;
        self.notify_end(b);
    }

    fn with<T>(f: impl FnOnce(&Self) -> T) -> T {
        thread_local!(static RT: Runtime = Runtime::new());
        RT.with(|data| f(data))
    }
    fn borrow_mut(&self) -> RefMut<RuntimeData> {
        ((self.0).0).0.borrow_mut()
    }
    fn notify_scope(&self) -> &NotifyScope {
        &(self.0).0
    }
    fn bind_scope(&self) -> &BindScope {
        &self.0
    }
}
