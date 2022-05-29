use futures_core::Future;
use rt_local_core::{spawn_local, Task};
use std::{
    pin::Pin,
    rc::{Rc, Weak},
    task::{Context, Poll},
};

pub fn spawn_local_weak_from<F: RcFuture<Output = ()> + 'static>(f: &Rc<F>) -> Task<()> {
    spawn_local_weak(Rc::downgrade(f))
}
pub fn spawn_local_weak<F: RcFuture<Output = ()> + 'static>(f: Weak<F>) -> Task<()> {
    let f: Weak<dyn RcFuture<Output = ()>> = f;
    spawn_local(WeakFuture(f))
}

pub trait RcFuture {
    type Output;
    fn poll(self: Rc<Self>, cx: &mut Context) -> Poll<Self::Output>;
}
struct WeakFuture(Weak<dyn RcFuture<Output = ()>>);

impl Future for WeakFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(f) = self.get_mut().0.upgrade() {
            f.poll(cx)
        } else {
            Poll::Ready(())
        }
    }
}
