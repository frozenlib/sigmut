use std::{
    cell::RefCell,
    ops::DerefMut,
    pin::Pin,
    rc::Weak,
    task::{Context, Poll},
};
use std::{future::Future, rc::Rc};

pub trait AsyncRuntime: 'static {
    fn spawn_local(&mut self, task: WeakAsyncTask) -> Box<dyn AsyncTaskHandle>;
}
pub trait AsyncTaskHandle: 'static {}

pub trait AsyncTask: 'static {
    fn poll(self: Rc<Self>, cx: &mut Context);
}

pub(crate) fn spawn_local_weak(task: &Rc<impl AsyncTask>) -> Box<dyn AsyncTaskHandle> {
    let task: Rc<dyn AsyncTask> = task.clone();
    let task = WeakAsyncTask(Rc::downgrade(&task));
    with_async_runtime(|rt| rt.spawn_local(task))
}

#[derive(Clone)]
pub struct WeakAsyncTask(Weak<dyn AsyncTask>);

impl Future for WeakAsyncTask {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<()> {
        if let Some(fut) = self.0.upgrade() {
            fut.poll(cx);
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}

thread_local! {
    static ASYNC_RUNTIME: RefCell<Option<Box<dyn AsyncRuntime>>> = RefCell::new(None);
}
pub(crate) fn with_async_runtime<T>(f: impl FnOnce(&mut dyn AsyncRuntime) -> T) -> T {
    ASYNC_RUNTIME.with(|rt| {
        f(rt.borrow_mut()
            .as_mut()
            .expect("async runtime is not set")
            .deref_mut())
    })
}
pub fn enter_async_runtime<T>(rt: impl AsyncRuntime, f: impl FnOnce() -> T) -> T {
    ASYNC_RUNTIME.with(|current| {
        let mut current = current.borrow_mut();
        if let Some(_) = *current {
            panic!("async runtime is already set");
        } else {
            *current = Some(Box::new(rt));
        }
    });
    let ret = f();
    ASYNC_RUNTIME.with(|current| current.take());
    ret
}
