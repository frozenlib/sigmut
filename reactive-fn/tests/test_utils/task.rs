use std::thread;
use std::time::Duration;

use futures::channel::oneshot::channel;

pub async fn sleep(duration: Duration) {
    let (sender, receiver) = channel();
    thread::spawn(move || {
        thread::sleep(duration);
        sender.send(()).unwrap();
    });
    receiver.await.unwrap();
}
