use std::time::Duration;

use reactive_fn::core::Runtime;
use tokio::time::sleep;

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
