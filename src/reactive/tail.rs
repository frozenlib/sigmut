use super::*;
use crate::bind::*;
use std::{cell::RefCell, iter::once, rc::Rc};

pub struct Tail<T: 'static> {
    source: Re<T>,
    state: Rc<RefCell<TailState>>,
}

impl<T> Tail<T> {
    pub(crate) fn new(source: Re<T>, scope: &BindContextScope) -> (T, Self) {
        let state = TailState::new();
        let value = state
            .borrow_mut()
            .bindings
            .update(scope, &state, |ctx| source.get(ctx));
        let this = Self { source, state };
        (value, this)
    }
    pub fn for_each(self, f: impl FnMut(T) + 'static) -> Subscription {
        self.fold(f, move |mut f, x| {
            f(x);
            f
        })
        .into()
    }
    pub fn fold<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, T) -> St + 'static,
    ) -> Fold<St> {
        let source = self.source;
        let fold = TailState::connect(self.state, initial_state, |s| {
            FoldBy::new_with_state(
                s,
                move |st, ctx| (f(st, source.get(ctx)), None),
                |(st, _)| st,
                |st| st,
            )
        });
        Fold(fold)
    }
    pub fn collect_to<E: Extend<T> + 'static>(self, e: E) -> Fold<E> {
        self.fold(e, |mut e, x| {
            e.extend(once(x));
            e
        })
    }
    pub fn collect<E: Extend<T> + Default + 'static>(self) -> Fold<E> {
        self.collect_to(Default::default())
    }
    pub fn to_vec(self) -> Fold<Vec<T>> {
        self.collect()
    }
}
pub struct TailRef<T: ?Sized + 'static> {
    source: ReRef<T>,
    state: Rc<RefCell<TailState>>,
}

impl<T: ?Sized + 'static> TailRef<T> {
    pub(crate) fn new(source: ReRef<T>, scope: &BindContextScope, f: impl FnOnce(&T)) -> Self {
        let state = TailState::new();
        state
            .borrow_mut()
            .bindings
            .update(scope, &state, |ctx| source.with(ctx, |_, value| f(value)));
        Self { source, state }
    }
    pub(crate) fn new_borrow<'a>(
        source: &'a ReBorrow<T>,
        scope: &'a BindContextScope,
    ) -> (Ref<'a, T>, Self) {
        let state = TailState::new();
        let b = state
            .borrow_mut()
            .bindings
            .update(scope, &state, |ctx| source.borrow(ctx));
        let this = Self {
            source: source.to_re_ref(),
            state,
        };
        (b, this)
    }

    pub fn for_each(self, f: impl FnMut(&T) + 'static) -> Subscription {
        self.fold(f, move |mut f, x| {
            f(x);
            f
        })
        .into()
    }
    pub fn fold<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, &T) -> St + 'static,
    ) -> Fold<St> {
        let source = self.source;
        let fold = TailState::connect(self.state, initial_state, |s| {
            FoldBy::new_with_state(
                s,
                move |st, ctx| (source.with(ctx, |_, value| f(st, value)), None),
                |(st, _)| st,
                |st| st,
            )
        });
        Fold(fold)
    }
    pub fn collect_to<E: for<'a> Extend<&'a T> + 'static>(self, e: E) -> Fold<E> {
        self.fold(e, |mut e, x| {
            e.extend(once(x));
            e
        })
    }
    pub fn collect<E: for<'a> Extend<&'a T> + Default + 'static>(self) -> Fold<E> {
        self.collect_to(Default::default())
    }
    pub fn to_vec(self) -> Fold<Vec<T>>
    where
        T: Copy,
    {
        self.collect()
    }
}

struct TailState {
    is_modified: bool,
    bindings: Bindings,
    sink: Option<Rc<dyn BindSink>>,
}
impl TailState {
    fn new() -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(TailState {
            is_modified: false,
            sink: None,
            bindings: Bindings::new(),
        }))
    }
    fn connect<U: BindSink, St>(
        this: Rc<RefCell<Self>>,
        initial_state: St,
        f: impl FnOnce(ScanState<(St, Option<Rc<RefCell<TailState>>>), St>) -> Rc<U>,
    ) -> Rc<U> {
        let mut b = this.borrow_mut();
        let s = if b.is_modified {
            ScanState::Unloaded(initial_state)
        } else {
            ScanState::Loaded((initial_state, Some(this.clone())))
        };
        let tail = f(s);
        if !b.is_modified {
            b.sink = Some(tail.clone());
        }
        tail
    }
}

impl BindSink for RefCell<TailState> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        let mut b = self.borrow_mut();
        b.is_modified = true;
        if let Some(sink) = b.sink.take() {
            sink.notify(ctx);
        }
    }
}
