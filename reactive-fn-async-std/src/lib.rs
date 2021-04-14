use reactive_fn::async_runtime::*;
use std::future::Future;

struct AsyncStdRuntime;
impl AsyncRuntime for AsyncStdRuntime {
    fn spawn_local(&mut self, task: WeakAsyncTask) -> AsyncTaskHandle {
        AsyncTaskHandle::new(async_std::task::spawn_local(task), |handle| {
            let _ = handle.cancel();
        })
    }
}
pub fn run<T>(f: impl Future<Output = T>) -> T {
    enter_async_runtime(AsyncStdRuntime, || async_std::task::block_on(f))
}
