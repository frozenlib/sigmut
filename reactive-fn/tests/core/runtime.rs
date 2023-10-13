use std::time::Duration;

use reactive_fn::{
    core::{wait_for_update, Runtime},
    Action, ObsCell,
};
use rt_local::runtime::core::test;
use tokio::time::sleep;

use crate::test_utils::code_path::{code, CodePathChecker};

#[test]
fn new() {
    let _rt = Runtime::new();
}

#[tokio::test]
async fn run() {
    let mut rt = Runtime::new();
    rt.run(|_| async {}).await;
}

#[tokio::test]
async fn run_sleep() {
    let mut rt = Runtime::new();
    rt.run(|_| async {
        sleep(Duration::from_millis(100)).await;
    })
    .await;
}

#[test]
async fn wait_for_update_empty() {
    let mut rt = Runtime::new();
    rt.run(|_| async {
        wait_for_update().await;
    })
    .await;
}

#[test]
async fn wait_for_update_subscribe() {
    let mut cp = CodePathChecker::new();
    let mut rt = Runtime::new();
    rt.run(|_| async {
        let x = ObsCell::new(10);
        let _s = x.obs().subscribe(|x| code(format!("get {x}")));
        code(1);
        wait_for_update().await;
        code(2);
    })
    .await;

    cp.expect(["1", "get 10", "2"]);
    cp.verify();
}

#[test]
async fn wait_for_update_action() {
    let mut cp = CodePathChecker::new();
    let mut rt = Runtime::new();
    rt.run(|_| async {
        let x = ObsCell::new(10);
        let _s = x.obs().subscribe(|x| code(format!("get {x}")));
        Action::new({
            let x = x.clone();
            move |ac| x.set(20, ac)
        })
        .schedule();

        code(1);
        wait_for_update().await;
        code(2);
    })
    .await;

    cp.expect(["1", "get 20", "2"]);
    cp.verify();
}
