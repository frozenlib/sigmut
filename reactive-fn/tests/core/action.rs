use std::{future::pending, time::Duration};

use futures::channel::oneshot::channel;
use reactive_fn::{core::Runtime, Action, AsyncActionContext};
use rt_local::{runtime::core::test, spawn_local};

use crate::test_utils::{
    code_path::{code, CodePathChecker},
    task::sleep,
};

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

#[test]
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

#[test]
async fn async_action_drop_at_runtime_drop() {
    struct UseCallOnDrop(AsyncActionContext);
    impl Drop for UseCallOnDrop {
        fn drop(&mut self) {
            code("action drop");
        }
    }

    let mut cp = CodePathChecker::new();
    let mut rt = Runtime::new();
    let action = Action::new_async(|ac| async move {
        let _s = UseCallOnDrop(ac);
        pending::<()>().await;
    });
    action.schedule();
    rt.run(|_| async {
        sleep(Duration::from_millis(100)).await;
    })
    .await;
    code("runtime drop");
    drop(rt);

    cp.expect(["runtime drop", "action drop"]);
    cp.verify();
}

// #[test]
// async fn available_call_in_drop_async() {
//     struct UseCallOnDrop(AsyncActionContext);
//     impl Drop for UseCallOnDrop {
//         fn drop(&mut self) {
//             self.0.call(|_ac| code("drop"));
//         }
//     }

//     let mut cp = CodePathChecker::new();
//     let mut rt = Runtime::new();
//     let action = Action::new_async(|ac| async move {
//         let _s = UseCallOnDrop(ac);
//         pending::<()>().await;
//     });
//     action.schedule();
//     rt.run(|_| async {
//         sleep(Duration::from_millis(100)).await;
//     })
//     .await;

//     cp.expect(["drop"]);
//     cp.verify();
// }

#[test]
async fn action_wake_runtime() {
    let mut cp = CodePathChecker::new();
    let mut rt = Runtime::new();
    let (sender, receiver) = channel();
    rt.run(|_| async {
        let _s = spawn_local(async {
            sleep(Duration::from_millis(1000)).await;
            Action::new(|_oc| {
                code("action");
                sender.send(()).unwrap();
            })
            .schedule();
        });

        receiver.await.unwrap();
    })
    .await;

    cp.expect("action");
    cp.verify();
}
