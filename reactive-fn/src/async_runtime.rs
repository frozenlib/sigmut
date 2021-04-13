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

pub trait DynWeakAsyncTask: 'static {
    fn poll(&self, cx: &mut Context);
}

#[derive(Clone)]
pub struct WeakAsyncTask(Weak<dyn DynWeakAsyncTask>);

impl WeakAsyncTask {
    pub fn from_rc(rc: Rc<impl DynWeakAsyncTask>) -> Self {
        let rc: Rc<dyn DynWeakAsyncTask> = rc;
        Self(Rc::downgrade(&rc))
    }
}

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
pub fn with_async_runtime<T>(f: impl FnOnce(&mut dyn AsyncRuntime) -> T) -> T {
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
