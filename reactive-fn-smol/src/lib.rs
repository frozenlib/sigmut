use extend::ext;
use reactive_fn::*;
use smol::LocalExecutor;
use std::{future::Future, task::Poll, thread::LocalKey};

struct LocalSpawner(&'static LocalKey<LocalExecutor<'static>>);

impl LocalSpawn for LocalSpawner {
    type Handle = smol::Task<()>;
    fn spawn_local(&self, fut: impl Future<Output = ()> + 'static) -> Self::Handle {
        self.0.with(|sp| sp.spawn(fut))
    }
}
pub fn spawner() -> impl LocalSpawn {
    LocalSpawner(&LOCAL_EXECUTOR)
}

thread_local! {
    pub static LOCAL_EXECUTOR: LocalExecutor<'static> = LocalExecutor::new();
}

#[ext(pub)]
impl<T: 'static> DynObs<T> {
    fn map_async<Fut>(&self, f: impl Fn(T) -> Fut + 'static) -> DynObsBorrow<Poll<Fut::Output>>
    where
        Fut: Future + 'static,
    {
        self.map_async_with(f, spawner())
    }

    fn for_each_async<Fut>(&self, f: impl FnMut(T) -> Fut + 'static) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        self.for_each_async_with(f, spawner())
    }
}

#[ext(pub)]
impl<T: 'static> ReRef<T> {
    fn map_async<Fut>(&self, f: impl Fn(&T) -> Fut + 'static) -> DynObsBorrow<Poll<Fut::Output>>
    where
        Fut: Future + 'static,
    {
        self.map_async_with(f, spawner())
    }

    fn for_each_async<Fut>(&self, f: impl FnMut(&T) -> Fut + 'static) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        self.for_each_async_with(f, spawner())
    }
}

#[ext(pub)]
impl<T: 'static> DynObsBorrow<T> {
    fn map_async<Fut>(&self, f: impl Fn(&T) -> Fut + 'static) -> DynObsBorrow<Poll<Fut::Output>>
    where
        Fut: Future + 'static,
    {
        self.map_async_with(f, spawner())
    }

    fn for_each_async<Fut>(&self, f: impl FnMut(&T) -> Fut + 'static) -> Subscription
    where
        Fut: Future<Output = ()> + 'static,
    {
        self.for_each_async_with(f, spawner())
    }
}
