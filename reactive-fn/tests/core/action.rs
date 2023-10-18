use std::{future::pending, time::Duration};

use futures::channel::oneshot::channel;
use reactive_fn::{core::Runtime, spawn_action, spawn_action_async, AsyncActionContext};
use rt_local::{runtime::core::test, spawn_local};

use crate::test_utils::{
    code_path::{code, CodePathChecker},
    task::sleep,
};

#[test]
fn test_spawn_action() {
    let mut cp = CodePathChecker::new();
    let mut rt = Runtime::new();
    spawn_action(|_| {
        code("action");
    });

    rt.update();

    cp.expect("action");
    cp.verify();
}

#[test]
async fn test_spawn_action_async() {
    let mut cp = CodePathChecker::new();
    let mut rt = Runtime::new();
    let (sender, receiver) = channel();
    spawn_action_async(|ac| async move {
        ac.call(|_ac| code("1"));
        sleep(Duration::from_millis(200)).await;
        ac.call(|_ac| code("2"));
        sender.send(()).unwrap();
    });
    rt.run(|_| async {
        receiver.await.unwrap();
    })
    .await;

    cp.expect(["1", "2"]);
    cp.verify();
}

#[test]
async fn async_action_drop_at_runtime_drop() {
    struct UseDrop(AsyncActionContext);
    impl Drop for UseDrop {
        fn drop(&mut self) {
            code("action drop");
        }
    }

    let mut cp = CodePathChecker::new();
    let mut rt = Runtime::new();
    spawn_action_async(|ac| async move {
        let _s = UseDrop(ac);
        pending::<()>().await;
    });
    rt.run(|_| async {
        sleep(Duration::from_millis(100)).await;
    })
    .await;
    code("runtime drop");
    drop(rt);

    cp.expect(["runtime drop", "action drop"]);
    cp.verify();
}

#[test]
async fn available_call_in_drop_async() {
    struct UseCallOnDrop(AsyncActionContext);
    impl Drop for UseCallOnDrop {
        fn drop(&mut self) {
            self.0.call(|_ac| code("drop"));
        }
    }

    let mut cp = CodePathChecker::new();
    let mut rt = Runtime::new();
    spawn_action_async(|ac| async move {
        let _s = UseCallOnDrop(ac);
        pending::<()>().await;
    });
    rt.run(|_| async {
        sleep(Duration::from_millis(100)).await;
    })
    .await;
    code("runtime drop");
    drop(rt);

    cp.expect(["runtime drop", "drop"]);
    cp.verify();
}

#[test]
async fn action_wake_runtime() {
    let mut cp = CodePathChecker::new();
    let mut rt = Runtime::new();
    let (sender, receiver) = channel();
    rt.run(|_| async {
        let _s = spawn_local(async {
            sleep(Duration::from_millis(1000)).await;
            spawn_action(|_oc| {
                code("action");
                sender.send(()).unwrap();
            });
        });

        receiver.await.unwrap();
    })
    .await;

    cp.expect("action");
    cp.verify();
}
