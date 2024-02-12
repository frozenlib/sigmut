use std::time::Duration;

use assert_call::{call, CallRecorder};
use reactive_fn::{
    core::{wait_for_update, Runtime},
    spawn_action, ObsCell,
};
use rt_local::runtime::core::test;

use crate::test_utils::task::sleep;

#[test]
fn new() {
    let _rt = Runtime::new();
}

#[test]
async fn run() {
    let mut rt = Runtime::new();
    rt.run(|_| async {}).await;
}

#[test]
async fn run_sleep() {
    let mut rt = Runtime::new();
    rt.run(|_| async {
        sleep(Duration::from_millis(100)).await;
    })
    .await;
}

#[test]
#[should_panic]
async fn wait_for_update_no_runtiem() {
    wait_for_update().await;
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
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    rt.run(|_| async {
        let x = ObsCell::new(10);
        let _s = x.obs().subscribe(|x| call!("get {x}"));
        call!("1");
        wait_for_update().await;
        call!("2");
    })
    .await;

    c.verify(["1", "get 10", "2"]);
}

#[test]
async fn wait_for_update_action() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    rt.run(|_| async {
        let x = ObsCell::new(10);
        let _s = x.obs().subscribe(|x| call!("get {x}"));
        spawn_action({
            let x = x.clone();
            move |ac| x.set(20, ac)
        });

        call!("1");
        wait_for_update().await;
        call!("2");
    })
    .await;

    c.verify(["1", "get 20", "2"]);
}
