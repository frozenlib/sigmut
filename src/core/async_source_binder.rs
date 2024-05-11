use std::{
    future::Future,
    pin::Pin,
    rc::Weak,
    task::{Context, Poll, Waker},
};

use super::{
    waker_from_sink, AsyncSignalContext, AsyncSignalContextSource, BindSink, Dirty,
    DirtyOrMaybeDirty, Slot, SourceBindings, UpdateContext,
};

const SLOT_WAKE: Slot = Slot(0);
const SLOT_DEPS: Slot = Slot(1);

pub struct AsyncSourceBinder {
    sc: AsyncSignalContextSource,
    sources: SourceBindings,
    dirty: Dirty,
    sink: Weak<dyn BindSink>,
    is_wake: bool,
    waker: Waker,
}
impl AsyncSourceBinder {
    pub fn new(sink: &Weak<impl BindSink>) -> Self {
        Self {
            sc: AsyncSignalContextSource::new(),
            sources: SourceBindings::new(),
            dirty: Dirty::Dirty,
            sink: sink.clone(),
            is_wake: false,
            waker: waker_from_sink(sink.clone(), SLOT_WAKE),
        }
    }
    pub fn is_clean(&self) -> bool {
        self.dirty.is_clean() && !self.is_wake
    }
    pub fn check(&mut self, uc: &mut UpdateContext) -> bool {
        self.sources.check_with(&mut self.dirty, uc)
    }
    pub fn init<T>(
        &mut self,
        f: impl FnOnce(AsyncSignalContext) -> T,
        uc: &mut UpdateContext,
    ) -> T {
        self.dirty = Dirty::Clean;
        self.is_wake = true;
        let asc = self.sc.sc();
        self.sources.update(
            self.sink.clone(),
            SLOT_DEPS,
            |sc| self.sc.with(sc, || f(asc)),
            uc,
        )
    }
    pub fn poll<T>(
        &mut self,
        fut: Pin<&mut impl Future<Output = T>>,
        uc: &mut UpdateContext,
    ) -> Poll<T> {
        self.is_wake = false;
        self.sources.update_with(
            self.sink.clone(),
            SLOT_DEPS,
            false,
            |sc| {
                self.sc
                    .with(sc, || fut.poll(&mut Context::from_waker(&self.waker)))
            },
            uc,
        )
    }
    pub fn clear(&mut self, uc: &mut UpdateContext) {
        self.sources.clear(uc);
        self.dirty = Dirty::Dirty;
    }

    pub fn on_notify(&mut self, slot: Slot, dirty: DirtyOrMaybeDirty) -> bool {
        let mut need_notify = false;
        match slot {
            SLOT_DEPS => {
                need_notify = self.dirty.is_clean();
                self.dirty |= dirty;
            }
            SLOT_WAKE => {
                need_notify = !self.is_wake;
                self.is_wake = true;
            }
            _ => {}
        }
        need_notify
    }
}
