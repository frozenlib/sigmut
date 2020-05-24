use crate::reactive::*;
use extend::ext;
use futures::Future;
use std::task::Poll;

#[derive(Default, Clone, Copy)]
pub struct LocalSpawner;

impl LocalSpawn for LocalSpawner {
    type Handle = smol::Task<()>;
    fn spawn_local(&self, fut: impl Future<Output = ()> + 'static) -> Self::Handle {
        smol::Task::local(fut)
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
