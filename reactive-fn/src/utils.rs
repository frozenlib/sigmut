use futures_core::Future;
use rt_local::{spawn_local, Task};
use std::{
    pin::Pin,
    rc::{Rc, Weak},
    task::{Context, Poll},
};

pub fn spawn_local_weak<F: RcFuture<Output = ()>>(f: &Rc<F>) -> Task<()> {
    let f: Rc<dyn RcFuture<Output = ()>> = f.clone();
    spawn_local(WeakRcFuture(Rc::downgrade(&f)))
}

pub trait RcFuture: 'static {
    type Output;
    fn poll(self: Rc<Self>, cx: &mut Context) -> Poll<Self::Output>;
}
struct WeakRcFuture(Weak<dyn RcFuture<Output = ()>>);

impl Future for WeakRcFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(f) = self.get_mut().0.upgrade() {
            f.poll(cx)
        } else {
            Poll::Ready(())
        }
    }
}
