use crate::reactive::*;
use extend::ext;
use futures::{
    future::{abortable, AbortHandle},
    Future,
};
use std::task::Poll;

pub struct AutoAbortHandle(AbortHandle);

impl Drop for AutoAbortHandle {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Default, Clone, Copy)]
pub struct LocalSpawner;

impl LocalSpawn for LocalSpawner {
    type Handle = AutoAbortHandle;
    fn spawn_local(&self, fut: impl Future<Output = ()> + 'static) -> Self::Handle {
        let (fut, handle) = abortable(fut);
        tokio::task::spawn_local(fut);
        AutoAbortHandle(handle)
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
