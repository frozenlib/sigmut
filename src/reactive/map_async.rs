use futures::future::RemoteHandle;
use futures::task::LocalSpawn;
use futures::task::LocalSpawnExt;
use std::any::Any;
use std::cell::RefCell;
use std::future::Future;
use std::mem::drop;
use std::mem::replace;
use std::rc::Rc;

use crate::binding::*;
use crate::reactive::*;

pub struct MapAsyncCacheData<S: Re, F, T, GetPendingValue, Spawn> {
    src: S,
    f: F,
    data: RefCell<Data<T>>,
    sinks: BindSinks,
    get_pending_value: GetPendingValue,
    spawn: Spawn,
}
struct Data<T> {
    value: Option<T>,
    state: State,
    binds: Bindings,
}

enum State {
    Invalid,
    Pending(RemoteHandle<()>),
    Valid,
}

impl<
        S: Re + 'static,
        F: Fn(S::Item) -> Fut + 'static,
        Fut: Future<Output = T> + 'static,
        T: 'static,
        GetPendingValue: Fn(Option<T>) -> T + 'static,
        Spawn: LocalSpawn + 'static,
    > MapAsyncCacheData<S, F, T, GetPendingValue, Spawn>
{
    pub fn new(src: S, f: F, get_pending_value: GetPendingValue, spawn: Spawn) -> Self {
        Self {
            src,
            f,
            data: RefCell::new(Data {
                value: None,
                state: State::Invalid,
                binds: Bindings::new(),
            }),
            sinks: BindSinks::new(),
            get_pending_value,
            spawn,
        }
    }

    fn modify(&self) {
        let mut d = self.data.borrow_mut();
        if let State::Valid = d.state {
            d.value = Some((self.get_pending_value)(replace(&mut d.value, None)));
        }
        d.state = State::Invalid;
    }
}

impl<
        S: Re + 'static,
        F: Fn(S::Item) -> Fut + 'static,
        Fut: Future<Output = T> + 'static,
        T: 'static,
        GetPendingValue: Fn(Option<T>) -> T + 'static,
        Spawn: LocalSpawn + 'static,
    > DynReRef<T> for MapAsyncCacheData<S, F, T, GetPendingValue, Spawn>
{
    fn dyn_borrow(&self, this: &dyn Any, ctx: &mut BindContext) -> Ref<T> {
        let this = Self::downcast(this);
        ctx.bind(this.clone());
        let mut d = self.data.borrow_mut();
        if let State::Invalid = d.state {
            let f = (self.f)(self.src.get(&mut d.binds.context(this.clone())));
            let this = this.clone();
            let handle = self
                .spawn
                .spawn_local_with_handle(async move {
                    let value = f.await;
                    let mut d = this.data.borrow_mut();
                    d.value = Some(value);
                    d.state = State::Valid;
                })
                .unwrap();
            d.state = State::Pending(handle);
        }
        if d.value.is_none() {
            d.value = Some((self.get_pending_value)(None));
        }
        drop(d);
        Ref::Cell(std::cell::Ref::map(self.data.borrow(), |d| {
            d.value.as_ref().unwrap()
        }))
    }
}

impl<
        S: Re + 'static,
        F: Fn(S::Item) -> Fut + 'static,
        Fut: Future<Output = T> + 'static,
        T: 'static,
        GetPendingValue: Fn(Option<T>) -> T + 'static,
        Spawn: LocalSpawn + 'static,
    > BindSource for MapAsyncCacheData<S, F, T, GetPendingValue, Spawn>
{
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}

impl<
        S: Re + 'static,
        F: Fn(S::Item) -> Fut + 'static,
        Fut: Future<Output = T> + 'static,
        T: 'static,
        GetPendingValue: Fn(Option<T>) -> T + 'static,
        Spawn: LocalSpawn + 'static,
    > BindSink for MapAsyncCacheData<S, F, T, GetPendingValue, Spawn>
{
    fn lock(&self) {
        self.sinks.lock();
    }
    fn unlock(self: Rc<Self>, modified: bool) {
        self.sinks.unlock_with(modified, || {
            self.modify();
        });
    }
}
