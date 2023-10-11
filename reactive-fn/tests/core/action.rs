use std::time::Duration;

use reactive_fn::{core::Runtime, Action};
use tokio::time::sleep;

use crate::test_utils::code_path::{code, CodePath, CodePathChecker};

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
    let action = Action::new_async_loose(|ac| async move {
        ac.call(|_ac| {
            code("1");
        });
        sleep(Duration::from_millis(100)).await;
        ac.call(|_ac| {
            code("2");
        });
    });
    action.schedule();
    rt.run(|_| async {
        sleep(Duration::from_millis(500)).await;
    })
    .await;

    cp.expect(["1", "2"]);
    cp.verify();
}

#[tokio::test]
async fn action_new_async_loose() {
    let mut cp = CodePathChecker::new();
    let mut rt = Runtime::new();
    let action = Action::new_async_loose(|ac| async move {
        ac.call(|_ac| {
            code("1");
        });
        sleep(Duration::from_millis(200)).await;
        ac.call(|_ac| {
            code("2");
        });
    });
    action.schedule();
    rt.run(|_| async {
        sleep(Duration::from_millis(500)).await;
    })
    .await;

    cp.expect(["1", "2"]);
    cp.verify();
}
