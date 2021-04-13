use async_std::task::JoinHandle;
use reactive_fn::async_runtime::*;
use std::future::Future;

struct AsyncStdRuntime;
struct AsyncStdTaskHandle(Option<JoinHandle<()>>);

impl AsyncRuntime for AsyncStdRuntime {
    fn spawn_local(
        &mut self,
        task: reactive_fn::async_runtime::WeakAsyncTask,
    ) -> Box<dyn reactive_fn::async_runtime::AsyncTaskHandle> {
        Box::new(AsyncStdTaskHandle(Some(async_std::task::spawn_local(task))))
    }
}
impl AsyncTaskHandle for AsyncStdTaskHandle {}
impl Drop for AsyncStdTaskHandle {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            let _ = handle.cancel();
        }
    }
}

pub fn run<T>(f: impl Future<Output = T>) -> T {
    enter_async_runtime(AsyncStdRuntime, || async_std::task::block_on(f))
}
