use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Waker},
};

use crate::{
    core::{
        waker_from_sink, AsyncSignalContext, AsyncSignalContextSource, BindSink, Dirty,
        DirtyOrMaybeDirty, NotifyContext, Slot, SourceBindings, Task, UpdateContext,
    },
    Scheduler, Subscription,
};

pub fn subscribe_async<Fut>(f: impl FnMut(AsyncSignalContext) -> Fut + 'static) -> Subscription
where
    Fut: Future<Output = ()> + 'static,
{
    let this = SubscribeAsyncNode::new(f, Scheduler::default());
    this.schedule();
    Subscription::from_rc(this)
}

const SLOT_DEPS: Slot = Slot(0);
const SLOT_WAKE: Slot = Slot(1);

struct SubscribeAsyncNodeData<GetFut, Fut> {
    scs: AsyncSignalContextSource,
    get_fut: GetFut,
    fut: Pin<Box<Option<Fut>>>,
    dirty: Dirty,
    is_wake: bool,
    sources: SourceBindings,
    waker: Waker,
}

struct SubscribeAsyncNode<GetFut, Fut> {
    data: RefCell<SubscribeAsyncNodeData<GetFut, Fut>>,
    scheduler: Scheduler,
}
impl<GetFut, Fut> SubscribeAsyncNode<GetFut, Fut>
where
    GetFut: FnMut(AsyncSignalContext) -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    fn new(f: GetFut, scheduler: Scheduler) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            data: RefCell::new(SubscribeAsyncNodeData {
                scs: AsyncSignalContextSource::new(),
                get_fut: f,
                fut: Box::pin(None),
                dirty: Dirty::Dirty,
                is_wake: false,
                sources: SourceBindings::new(),
                waker: waker_from_sink(this.clone(), SLOT_WAKE),
            }),
            scheduler,
        })
    }
    fn schedule(self: &Rc<Self>) {
        Task::from_weak_fn(Rc::downgrade(self), |this, uc| this.call(uc))
            .schedule_with(&self.scheduler)
    }
    fn call(self: &Rc<Self>, uc: &mut UpdateContext) {
        let d = &mut *self.data.borrow_mut();
        if d.dirty.check(&mut d.sources, uc) {
            let sink = Rc::downgrade(self);
            d.fut.set(None);
            d.fut.set(d.sources.update(
                sink,
                SLOT_DEPS,
                true,
                |sc| Some(d.scs.with(sc, || (d.get_fut)(d.scs.sc()))),
                uc,
            ));
            d.dirty = Dirty::Clean;
            d.is_wake = true;
        }
        if d.is_wake {
            if let Some(f) = d.fut.as_mut().as_pin_mut() {
                let sink = Rc::downgrade(self);
                let value = d.sources.update(
                    sink,
                    SLOT_DEPS,
                    false,
                    |sc| {
                        d.scs
                            .with(sc, || f.poll(&mut Context::from_waker(&d.waker)))
                    },
                    uc,
                );
                if value.is_ready() {
                    d.fut.set(None);
                }
            }
            d.is_wake = false;
        }
    }
}
impl<F, Fut> BindSink for SubscribeAsyncNode<F, Fut>
where
    F: FnMut(AsyncSignalContext) -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    fn notify(self: Rc<Self>, slot: Slot, dirty: DirtyOrMaybeDirty, _nc: &mut NotifyContext) {
        let mut d = self.data.borrow_mut();
        let need_schedule = d.dirty.is_clean() && !d.is_wake;
        match slot {
            SLOT_DEPS => d.dirty |= dirty,
            SLOT_WAKE => d.is_wake = true,
            _ => {}
        }
        if need_schedule {
            self.schedule();
        }
    }
}
