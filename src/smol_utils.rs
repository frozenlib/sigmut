use crate::LocalSpawn;
use futures::Future;

#[derive(Default, Clone, Copy)]
pub struct LocalSpawner;

impl LocalSpawn for LocalSpawner {
    type Handle = smol::Task<()>;
    fn spawn_local(&self, fut: impl Future<Output = ()> + 'static) -> Self::Handle {
        smol::Task::local(fut)
    }
}
