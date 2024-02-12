use std::{future::pending, time::Duration};

use assert_call::{call, CallRecorder};
use futures::channel::oneshot::channel;
use reactive_fn::{core::Runtime, spawn_action, spawn_action_async, AsyncActionContext};
use rt_local::{runtime::core::test, spawn_local};

use crate::test_utils::task::sleep;

#[test]
fn test_spawn_action() {
    let mut cp = CallRecorder::new();
    let mut rt = Runtime::new();
    spawn_action(|_| {
        call!("action");
    });

    rt.update();

    cp.verify("action");
}

#[test]
async fn test_spawn_action_async() {
    let mut cp = CallRecorder::new();
    let mut rt = Runtime::new();
    let (sender, receiver) = channel();
    spawn_action_async(|ac| async move {
        ac.call(|_ac| call!("1"));
        sleep(Duration::from_millis(200)).await;
        ac.call(|_ac| call!("2"));
        sender.send(()).unwrap();
    });
    rt.run(|_| async {
        receiver.await.unwrap();
    })
    .await;

    cp.verify(["1", "2"]);
}

#[test]
async fn async_action_drop_at_runtime_drop() {
    struct UseDrop(AsyncActionContext);
    impl Drop for UseDrop {
        fn drop(&mut self) {
            call!("action drop");
        }
    }

    let mut cp = CallRecorder::new();
    let mut rt = Runtime::new();
    spawn_action_async(|ac| async move {
        let _s = UseDrop(ac);
        pending::<()>().await;
    });
    rt.run(|_| async {
        sleep(Duration::from_millis(100)).await;
    })
    .await;
    call!("runtime drop");
    drop(rt);

    cp.verify(["runtime drop", "action drop"]);
}

#[test]
async fn available_call_in_drop_async() {
    struct UseCallOnDrop(AsyncActionContext);
    impl Drop for UseCallOnDrop {
        fn drop(&mut self) {
            self.0.call(|_ac| call!("drop"));
        }
    }

    let mut cp = CallRecorder::new();
    let mut rt = Runtime::new();
    spawn_action_async(|ac| async move {
        let _s = UseCallOnDrop(ac);
        pending::<()>().await;
    });
    rt.run(|_| async {
        sleep(Duration::from_millis(100)).await;
    })
    .await;
    call!("runtime drop");
    drop(rt);

    cp.verify(["runtime drop", "drop"]);
}

#[test]
async fn action_wake_runtime() {
    let mut cp = CallRecorder::new();
    let mut rt = Runtime::new();
    let (sender, receiver) = channel();
    rt.run(|_| async {
        let _s = spawn_local(async {
            sleep(Duration::from_millis(1000)).await;
            spawn_action(|_oc| {
                call!("action");
                sender.send(()).unwrap();
            });
        });

        receiver.await.unwrap();
    })
    .await;

    cp.verify("action");
}
