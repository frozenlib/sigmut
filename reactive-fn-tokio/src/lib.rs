use reactive_fn::async_runtime::*;
use std::future::Future;
use tokio::task::JoinHandle;

struct TokioAsyncRuntime;
struct TokioAsyncHandle(JoinHandle<()>);

impl AsyncRuntime for TokioAsyncRuntime {
    fn spawn_local(&mut self, task: WeakAsyncTask) -> Box<dyn AsyncTaskHandle> {
        Box::new(TokioAsyncHandle(tokio::task::spawn_local(task)))
    }
}
impl AsyncTaskHandle for TokioAsyncHandle {}
impl Drop for TokioAsyncHandle {
    fn drop(&mut self) {
        self.0.abort()
    }
}

pub fn run<T>(future: impl Future<Output = T>) -> T {
    let mut rt = tokio::runtime::Runtime::new().unwrap();
    let local = tokio::task::LocalSet::new();
    enter_async_runtime(TokioAsyncRuntime, || local.block_on(&mut rt, future))
}
