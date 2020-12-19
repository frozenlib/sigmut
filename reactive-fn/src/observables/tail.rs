use super::*;
use crate::bind::*;
use std::{cell::RefCell, iter::once, rc::Rc};

pub struct Tail<T: 'static>(TailOps<DynObs<T>>);

impl<T> Tail<T> {
    pub(super) fn new(source: DynObs<T>, scope: &BindScope) -> (T, Self) {
        let (value, s) = TailOps::new(source, scope);
        (value, Self(s))
    }
    pub fn empty() -> Self {
        Self(TailOps::empty())
    }

    pub fn for_each(self, f: impl FnMut(T) + 'static) -> Subscription {
        self.0.for_each(f)
    }
    pub fn fold<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, T) -> St + 'static,
    ) -> Fold<St> {
        self.0.fold(initial_state, f)
    }
    pub fn collect_to<E: Extend<T> + 'static>(self, e: E) -> Fold<E> {
        self.0.collect_to(e)
    }
    pub fn collect<E: Extend<T> + Default + 'static>(self) -> Fold<E> {
        self.0.collect()
    }
    pub fn collect_vec(self) -> Fold<Vec<T>> {
        self.0.collect_vec()
    }
}

pub struct TailOps<S>(Option<TailData<S>>);

impl<S: Observable> TailOps<S> {
    pub(super) fn new(source: S, scope: &BindScope) -> (S::Item, Self) {
        let state = TailState::new();
        let mut b = state.borrow_mut();
        let value = b.bindings.update(scope, &state, |cx| source.get(cx));
        let data = if b.bindings.is_empty() {
            None
        } else {
            drop(b);
            Some(TailData { source, state })
        };
        (value, Self(data))
    }
    pub fn empty() -> Self {
        Self(None)
    }

    pub fn for_each(self, f: impl FnMut(S::Item) + 'static) -> Subscription {
        self.fold(f, move |mut f, x| {
            f(x);
            f
        })
        .into()
    }
    pub fn fold<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, S::Item) -> St + 'static,
    ) -> Fold<St> {
        if let Some(this) = self.0 {
            let source = this.source;
            let fold = TailState::connect(this.state, initial_state, |s| {
                FoldBy::new_with_state(
                    s,
                    fold_by_op(
                        move |st, cx| (f(st, source.get(cx)), None),
                        |(st, _)| st,
                        |(st, _)| st,
                    ),
                )
            });
            Fold::new(fold)
        } else {
            Fold::constant(initial_state)
        }
    }
    pub fn collect_to<E: Extend<S::Item> + 'static>(self, e: E) -> Fold<E> {
        self.fold(e, |mut e, x| {
            e.extend(once(x));
            e
        })
    }
    pub fn collect<E: Extend<S::Item> + Default + 'static>(self) -> Fold<E> {
        self.collect_to(Default::default())
    }
    pub fn collect_vec(self) -> Fold<Vec<S::Item>> {
        self.collect()
    }
}

pub struct TailRef<T: ?Sized + 'static>(TailRefOps<ReRef<T>>);

impl<T: ?Sized + 'static> TailRef<T> {
    pub(super) fn new(source: ReRef<T>, scope: &BindScope, f: impl FnOnce(&T)) -> Self {
        Self(TailRefOps::new(source, scope, f))
    }
    pub(super) fn new_borrow<'a>(
        source: &'a DynObsBorrow<T>,
        scope: &'a BindScope,
    ) -> (Ref<'a, T>, Self) {
        let (r, s) = TailRefOps::new_borrow(source, scope, |s| s.as_ref());
        (r, Self(s))
    }
    pub fn empty() -> Self {
        Self(TailRefOps::empty())
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
        self.0.fold(initial_state, f)
    }
    pub fn collect_to<E: for<'a> Extend<&'a T> + 'static>(self, e: E) -> Fold<E> {
        self.0.collect_to(e)
    }
    pub fn collect<E: for<'a> Extend<&'a T> + Default + 'static>(self) -> Fold<E> {
        self.0.collect()
    }
    pub fn collect_vec(self) -> Fold<Vec<T>>
    where
        T: Copy,
    {
        self.0.collect_vec()
    }
}

pub struct TailRefOps<S>(Option<TailData<S>>);

impl<S: ObservableRef> TailRefOps<S> {
    pub(super) fn new(source: S, scope: &BindScope, f: impl FnOnce(&S::Item)) -> Self {
        let state = TailState::new();
        let mut b = state.borrow_mut();
        b.bindings
            .update(scope, &state, |cx| source.with(|value, _| f(value), cx));
        if b.bindings.is_empty() {
            Self(None)
        } else {
            drop(b);
            Self(Some(TailData { source, state }))
        }
    }
    pub(super) fn new_borrow<'a, B: ObservableBorrow<Item = S::Item>>(
        source: &'a B,
        scope: &'a BindScope,
        to_ref: impl Fn(&B) -> S,
    ) -> (Ref<'a, B::Item>, Self) {
        let state = TailState::new();
        let mut b = state.borrow_mut();
        let r = b.bindings.update(scope, &state, |cx| source.borrow(cx));
        let this = if b.bindings.is_empty() {
            TailRefOps(None)
        } else {
            drop(b);
            TailRefOps(Some(TailData {
                source: to_ref(&source),
                state,
            }))
        };
        (r, this)
    }
    pub fn empty() -> Self {
        Self(None)
    }

    pub fn for_each(self, f: impl FnMut(&S::Item) + 'static) -> Subscription {
        self.fold(f, move |mut f, x| {
            f(x);
            f
        })
        .into()
    }
    pub fn fold<St: 'static>(
        self,
        initial_state: St,
        f: impl Fn(St, &S::Item) -> St + 'static,
    ) -> Fold<St> {
        if let Some(this) = self.0 {
            let source = this.source;
            let fold = TailState::connect(this.state, initial_state, |s| {
                FoldBy::new_with_state(
                    s,
                    fold_by_op(
                        move |st, cx| (source.with(|value, _| f(st, value), cx), None),
                        |(st, _)| st,
                        |(st, _)| st,
                    ),
                )
            });
            Fold::new(fold)
        } else {
            Fold::constant(initial_state)
        }
    }
    pub fn collect_to<E: for<'a> Extend<&'a S::Item> + 'static>(self, e: E) -> Fold<E> {
        self.fold(e, |mut e, x| {
            e.extend(once(x));
            e
        })
    }
    pub fn collect<E: for<'a> Extend<&'a S::Item> + Default + 'static>(self) -> Fold<E> {
        self.collect_to(Default::default())
    }
    pub fn collect_vec(self) -> Fold<Vec<S::Item>>
    where
        S::Item: Copy,
    {
        self.collect()
    }
}

struct TailData<S> {
    source: S,
    state: Rc<RefCell<TailState>>,
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
    fn notify(self: Rc<Self>, scope: &NotifyScope) {
        let mut b = self.borrow_mut();
        b.is_modified = true;
        if let Some(sink) = b.sink.take() {
            sink.notify(scope);
        }
    }
}
