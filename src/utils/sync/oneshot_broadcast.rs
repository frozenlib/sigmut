use std::{
    future::poll_fn,
    mem::take,
    sync::{Arc, Mutex},
    task::{Poll, Waker},
};

use derive_ex::Ex;
use slabmap::SlabMap;

struct OneshotBroadcast<T> {
    value: Option<T>,
    waker: SlabMap<Waker>,
}

pub fn oneshot_broadcast<T>() -> (Sender<T>, Receiver<T>) {
    let data = Arc::new(Mutex::new(OneshotBroadcast {
        value: None,
        waker: SlabMap::new(),
    }));
    (Sender(data.clone()), Receiver(data))
}

pub struct Sender<T>(Arc<Mutex<OneshotBroadcast<T>>>);

impl<T> Sender<T> {
    pub fn send(&self, value: T) {
        let mut data = self.0.lock().unwrap();
        data.value = Some(value);
        for (_, waker) in take(&mut data.waker) {
            waker.wake();
        }
    }
}

#[derive(Ex)]
#[derive_ex(Clone(bound()))]
pub struct Receiver<T>(Arc<Mutex<OneshotBroadcast<T>>>);

impl<T: Clone> Receiver<T> {
    pub async fn recv(&self) -> T {
        let mut key = WakerKeyGuard::new(self);
        poll_fn(|cx| {
            let mut d = self.0.lock().unwrap();
            if let Some(value) = &d.value {
                Poll::Ready(value.clone())
            } else {
                if let Some(key) = key.key {
                    d.waker[key].clone_from(cx.waker());
                } else {
                    key.key = Some(d.waker.insert(cx.waker().clone()));
                }
                Poll::Pending
            }
        })
        .await
    }
}
struct WakerKeyGuard<'a, T> {
    receiver: &'a Receiver<T>,
    key: Option<usize>,
}
impl<'a, T> WakerKeyGuard<'a, T> {
    fn new(receiver: &'a Receiver<T>) -> Self {
        Self {
            receiver,
            key: None,
        }
    }
}
impl<T> Drop for WakerKeyGuard<'_, T> {
    fn drop(&mut self) {
        if let Some(key) = self.key {
            self.receiver.0.lock().unwrap().waker.remove(key);
        }
    }
}


