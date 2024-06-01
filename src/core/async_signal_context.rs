use std::{
    cell::RefCell,
    future::{poll_fn, Future},
    pin::Pin,
    rc::{Rc, Weak},
    task::{Context, Poll, Waker},
};

use bumpalo::Bump;

use super::{
    waker_from_sink, BindSink, Dirty, DirtyOrMaybeDirty, RawRuntime, SignalContext, Sink, Slot,
    SourceBindings, UpdateContext,
};

const SLOT_WAKE: Slot = Slot(0);
const SLOT_DEPS: Slot = Slot(1);
const SLOT_POLL: Slot = Slot(2);

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
struct SignalContextPtr {
    rt: *mut RawRuntime,
    bump: *const Bump,
    sink: Option<*mut Sink>,
}
impl SignalContextPtr {
    fn new(sc: &mut SignalContext) -> Self {
        Self {
            rt: sc.rt,
            bump: sc.bump,
            sink: sc.sink.as_mut().map(|x| *x as *mut _),
        }
    }
    unsafe fn sc(&self) -> SignalContext {
        SignalContext {
            rt: &mut *self.rt,
            bump: &*self.bump,
            sink: self.sink.map(|x| &mut *x),
        }
    }
}
unsafe fn sc(p: &mut Option<SignalContextPtr>) -> SignalContext {
    if let Some(p) = p {
        unsafe { p.sc() }
    } else {
        panic!("`SignalContext` cannot be used after being moved.");
    }
}

#[derive(Default)]
struct AsyncSignalContextState {
    sc: Option<SignalContextPtr>,
    poll_waker: Option<Waker>,
    poll_bindings: SourceBindings,
}

struct AsyncSignalContextData {
    s: RefCell<AsyncSignalContextState>,
    sink: Weak<dyn BindSink>,
}

/// Context for asynchronous state retrieval and dependency tracking.
pub struct AsyncSignalContext(Rc<AsyncSignalContextData>);

impl AsyncSignalContext {
    pub fn with<T>(&mut self, f: impl FnOnce(&mut SignalContext) -> T) -> T {
        unsafe { f(&mut sc(&mut self.0.s.borrow_mut().sc)) }
    }

    /// Creates a future that wraps a signal function returning [`Poll`].
    ///
    /// If `f` returns `Pending`, `f` is called again when the dependencies recorded in the `SignalContext` change.
    ///
    /// If `f` returns `Ready`, the dependencies recorded in `SignalContext` are added to the dependencies in `AsyncSignalContext`,
    /// and the asynchronous function is completed.
    ///
    /// Only the dependencies recorded in `SingalContext` in the last call of `f` are added to `AsyncSignalContext` dependencies.
    pub async fn poll_fn<T>(&mut self, f: impl Fn(&mut SignalContext) -> Poll<T>) -> T {
        poll_fn(|cx| {
            let s = &mut *self.0.s.borrow_mut();
            let mut sc = unsafe { sc(&mut s.sc) };

            let sink = self.0.sink.clone();
            let ret = s.poll_bindings.update(sink, SLOT_POLL, true, &f, sc.uc());
            if ret.is_ready() {
                sc.extend(&mut s.poll_bindings);
                s.poll_waker.take();
            } else {
                s.poll_waker = Some(cx.waker().clone());
            }
            ret
        })
        .await
    }
}

struct AsyncSignalContextSource(Rc<AsyncSignalContextData>);

impl AsyncSignalContextSource {
    pub fn new(sink: Weak<dyn BindSink>) -> Self {
        Self(Rc::new(AsyncSignalContextData {
            s: RefCell::new(AsyncSignalContextState::default()),
            sink,
        }))
    }
    pub fn sc(&self) -> AsyncSignalContext {
        AsyncSignalContext(self.0.clone())
    }
    pub fn with<T>(&self, sc: &mut SignalContext, f: impl FnOnce() -> T) -> T {
        let data = SignalContextPtr::new(sc);
        assert!(self.0.s.borrow().sc.is_none());
        self.0.s.borrow_mut().sc = Some(data);
        let ret = f();
        assert!(self.0.s.borrow().sc == Some(data));
        self.0.s.borrow_mut().sc = None;
        ret
    }
}

pub struct AsyncSourceBinder {
    sc: AsyncSignalContextSource,
    sources: SourceBindings,
    dirty: Dirty,
    is_wake: bool,
    waker: Waker,
}
impl AsyncSourceBinder {
    pub fn new(sink: &Weak<impl BindSink>) -> Self {
        Self {
            sc: AsyncSignalContextSource::new(sink.clone()),
            sources: SourceBindings::new(),
            dirty: Dirty::Dirty,
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
        let sink = self.sc.0.sink.clone();
        self.sources
            .update(sink, SLOT_DEPS, true, |sc| self.sc.with(sc, || f(asc)), uc)
    }
    pub fn poll<T>(
        &mut self,
        fut: Pin<&mut impl Future<Output = T>>,
        uc: &mut UpdateContext,
    ) -> Poll<T> {
        self.is_wake = false;
        let sink = self.sc.0.sink.clone();
        self.sources.update(
            sink,
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
            SLOT_WAKE => {
                need_notify = !self.is_wake;
                self.is_wake = true;
            }
            SLOT_DEPS => {
                need_notify = self.dirty.is_clean();
                self.dirty |= dirty;
            }
            SLOT_POLL => {
                if let Some(waker) = self.sc.0.s.borrow_mut().poll_waker.take() {
                    waker.wake();
                }
            }
            _ => {}
        }
        need_notify
    }
}
