use reactive_fn::async_runtime::*;
use std::future::Future;

struct TokioAsyncRuntime;

impl AsyncRuntime for TokioAsyncRuntime {
    fn spawn_local(&mut self, task: WeakAsyncTask) -> AsyncTaskHandle {
        AsyncTaskHandle::new(tokio::task::spawn_local(task), |handle| {
            handle.abort();
        })
    }
}

pub fn run<T>(future: impl Future<Output = T>) -> T {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let local = tokio::task::LocalSet::new();
    enter_async_runtime(TokioAsyncRuntime, || local.block_on(&rt, future))
}
