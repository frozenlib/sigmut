use crate::{
    core::{schedule_update, BindSink, CallUpdate, Computed, SourceBindings, UpdateContext},
    ObsContext,
};
use futures::Stream;
use std::{
    cell::RefCell,
    mem::take,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

pub fn stream<T: 'static>(
    f: impl FnMut(&mut ObsContext) -> T + 'static,
) -> impl Stream<Item = T> + Unpin + 'static {
    ObsStream::new(f)
}

const SLOT: usize = 0;

struct Data<F, T> {
    f: F,
    is_scheduled_update: bool,
    computed: Computed,
    value: ValueState<T>,
    bindings: SourceBindings,
}

#[derive(Default)]
enum ValueState<T> {
    #[default]
    None,
    Pending(Waker),
    Ready(T),
}

struct RawObsStream<F, T>(RefCell<Data<F, T>>);

struct ObsStream<F, T>(Rc<RawObsStream<F, T>>);

impl<F, T> ObsStream<F, T> {
    pub fn new(f: F) -> Self {
        Self(Rc::new(RawObsStream(RefCell::new(Data {
            f,
            is_scheduled_update: false,
            computed: Computed::None,
            value: ValueState::None,
            bindings: SourceBindings::new(),
        }))))
    }
}

impl<F, T> Stream for ObsStream<F, T>
where
    F: FnMut(&mut ObsContext) -> T + 'static,
    T: 'static,
{
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut d = self.0 .0.borrow_mut();
        match take(&mut d.value) {
            ValueState::None | ValueState::Pending(_) => {
                d.value = ValueState::Pending(cx.waker().clone());
                if d.computed != Computed::UpToDate && !d.is_scheduled_update {
                    d.is_scheduled_update = true;
                    let node = Rc::downgrade(&self.as_ref().0);
                    schedule_update(node, SLOT);
                }
                Poll::Pending
            }
            ValueState::Ready(value) => Poll::Ready(Some(value)),
        }
    }
}

impl<F, T> BindSink for RawObsStream<F, T>
where
    F: FnMut(&mut ObsContext) -> T + 'static,
    T: 'static,
{
    fn notify(self: Rc<Self>, _slot: usize, is_modified: bool, uc: &mut UpdateContext) {
        let mut is_schedule = false;
        if let Ok(mut d) = self.0.try_borrow_mut() {
            if d.computed.modify(is_modified) && !d.is_scheduled_update {
                d.is_scheduled_update = true;
                is_schedule = true;
            }
        }
        if is_schedule {
            uc.schedule_update(self, SLOT);
        }
    }
}

impl<F, T> CallUpdate for RawObsStream<F, T>
where
    F: FnMut(&mut ObsContext) -> T + 'static,
    T: 'static,
{
    fn call_update(self: Rc<Self>, _slot: usize, uc: &mut UpdateContext) {
        let mut d = self.0.borrow_mut();
        let d = &mut *d;
        d.is_scheduled_update = false;
        if d.computed == Computed::MayBeOutdated {
            if d.bindings.flush(uc) {
                d.computed = Computed::Outdated;
            } else {
                d.computed = Computed::UpToDate;
            }
        }
        if d.computed != Computed::UpToDate {
            d.computed = Computed::UpToDate;
            let node = Rc::downgrade(&self);
            let value = d.bindings.compute(node, SLOT, |oc| (d.f)(oc.reset()), uc);
            let waker = if let ValueState::Pending(waker) = take(&mut d.value) {
                Some(waker)
            } else {
                None
            };
            d.value = ValueState::Ready(value);
            if let Some(waker) = waker {
                waker.wake();
            }
        }
    }
}
