use async_std::task::{spawn_local, JoinHandle};
use extend::ext;
use futures::Future;
use reactive_fn::*;
use std::task::Poll;

pub struct AutoCancelHandle(Option<JoinHandle<()>>);

impl Drop for AutoCancelHandle {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            spawn_local(handle.cancel());
        }
    }
}
#[derive(Default, Clone, Copy)]
pub struct LocalSpawner;

impl LocalSpawn for LocalSpawner {
    type Handle = AutoCancelHandle;
    fn spawn_local(&self, fut: impl Future<Output = ()> + 'static) -> Self::Handle {
        AutoCancelHandle(Some(spawn_local(fut)))
    }
}

#[ext(pub)]
impl<T: 'static> Re<T> {
    fn map_async<Fut>(&self, f: impl Fn(T) -> Fut + 'static) -> ReBorrow<Poll<Fut::Output>>
    where
        Fut: Future + 'static,
    {
        self.map_async_with(f, LocalSpawner)
    }

    fn for_each_async<Fut>(&self, f: impl FnMut(T) -> Fut + 'static) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        self.for_each_async_with(f, LocalSpawner)
    }
}

#[ext(pub)]
impl<T: 'static> ReRef<T> {
    fn map_async<Fut>(&self, f: impl Fn(&T) -> Fut + 'static) -> ReBorrow<Poll<Fut::Output>>
    where
        Fut: Future + 'static,
    {
        self.map_async_with(f, LocalSpawner)
    }

    fn for_each_async<Fut>(&self, f: impl FnMut(&T) -> Fut + 'static) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        self.for_each_async_with(f, LocalSpawner)
    }
}

#[ext(pub)]
impl<T: 'static> ReBorrow<T> {
    fn map_async<Fut>(&self, f: impl Fn(&T) -> Fut + 'static) -> ReBorrow<Poll<Fut::Output>>
    where
        Fut: Future + 'static,
    {
        self.map_async_with(f, LocalSpawner)
    }

    fn for_each_async<Fut>(&self, f: impl FnMut(&T) -> Fut + 'static) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        self.for_each_async_with(f, LocalSpawner)
    }
}
