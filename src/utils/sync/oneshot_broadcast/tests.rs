use super::*;
use std::{
    future::Future,
    sync::atomic::{AtomicBool, Ordering},
    task::{Context, Wake},
};

struct CheckLock {
    channel: Arc<Mutex<OneshotBroadcast<usize>>>,
    outside: Arc<AtomicBool>,
}
impl CheckLock {
    fn check(&self) {
        self.outside
            .fetch_and(self.channel.try_lock().is_ok(), Ordering::Relaxed);
    }
}
impl Wake for CheckLock {
    fn wake(self: Arc<Self>) {
        self.check();
    }
}
impl Drop for CheckLock {
    fn drop(&mut self) {
        self.check();
    }
}

#[rstest::rstest]
fn waker_callbacks_run_outside_mutex(#[values("send", "replace", "cancel")] operation: &str) {
    let (sender, receiver) = oneshot_broadcast();
    let outside = Arc::new(AtomicBool::new(true));
    let mut future = Box::pin(receiver.recv());
    let waker = Waker::from(Arc::new(CheckLock {
        channel: sender.0.clone(),
        outside: outside.clone(),
    }));
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_pending()
    );
    drop(waker);
    match operation {
        "send" => sender.send(1),
        "replace" => {
            assert!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
        }
        "cancel" => drop(future),
        _ => unreachable!(),
    }
    assert!(outside.load(Ordering::Relaxed));
}
