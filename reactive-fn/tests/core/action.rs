use std::time::Duration;

use futures::channel::oneshot::channel;
use reactive_fn::{core::Runtime, Action};
use tokio::time::sleep;

use crate::test_utils::code_path::{code, CodePathChecker};

#[test]
fn action_new() {
    let mut cp = CodePathChecker::new();
    let mut rt = Runtime::new();
    let action = Action::new(|_| {
        code("action");
    });
    action.schedule();
    rt.update();

    cp.expect("action");
    cp.verify();
}

#[tokio::test]
async fn action_new_async() {
    let mut cp = CodePathChecker::new();
    let mut rt = Runtime::new();
    let (sender, receiver) = channel();
    let action = Action::new_async(|ac| async move {
        ac.call(|_ac| code("1"));
        sleep(Duration::from_millis(200)).await;
        ac.call(|_ac| code("2"));
        sender.send(()).unwrap();
    });
    action.schedule();
    rt.run(|_| async {
        receiver.await.unwrap();
    })
    .await;

    cp.expect(["1", "2"]);
    cp.verify();
}
