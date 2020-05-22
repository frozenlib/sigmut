use crate::reactive::*;
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

pub trait ReExt<T> {
    fn map_async<Fut>(&self, f: impl Fn(T) -> Fut + 'static) -> ReBorrow<Poll<Fut::Output>>
    where
        Fut: Future + 'static;
}
impl<T: 'static> ReExt<T> for Re<T> {
    fn map_async<Fut>(&self, f: impl Fn(T) -> Fut + 'static) -> ReBorrow<Poll<Fut::Output>>
    where
        Fut: Future + 'static,
    {
        self.map_async_with(f, LocalSpawner)
    }
}
