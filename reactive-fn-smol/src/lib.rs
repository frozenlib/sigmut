use async_executor::LocalExecutor;
use futures::executor::block_on;
use reactive_fn::async_runtime::*;
use std::{future::Future, thread::LocalKey};

struct SmolAsyncRuntime(&'static LocalKey<LocalExecutor<'static>>);

impl AsyncRuntime for SmolAsyncRuntime {
    fn spawn_local(&mut self, task: WeakAsyncTask) -> AsyncTaskHandle {
        AsyncTaskHandle::new(self.0.with(|ex| ex.spawn(task)), |_| {})
    }
}

pub fn run<T>(
    executor: &'static LocalKey<LocalExecutor<'static>>,
    future: impl Future<Output = T>,
) -> T {
    enter_async_runtime(SmolAsyncRuntime(executor), || {
        executor.with(|ex| block_on(ex.run(future)))
    })
}
