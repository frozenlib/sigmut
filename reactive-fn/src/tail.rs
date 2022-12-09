use crate::*;
use std::{cell::RefCell, iter::once, mem, rc::Rc};

pub struct DynTail<T: ?Sized + 'static>(Tail<Obs<T>>);

impl<T: ?Sized + 'static> DynTail<T> {
    pub(super) fn new<U>(source: Obs<T>, scope: &BindScope, f: impl FnOnce(&T) -> U) -> (U, Self) {
        let (head, tail) = Tail::new(source, scope, f);
        (head, Self(tail))
    }
    pub fn empty() -> Self {
        Self(Tail::empty())
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn fold<St: 'static>(
        self,
        initial_state: St,
        f: impl FnMut(&mut St, &T) + 'static,
    ) -> Fold<St> {
        self.0.fold(initial_state, f)
    }
    pub fn collect_to<E>(self, e: E) -> Fold<E>
    where
        T: ToOwned,
        E: Extend<<T as ToOwned>::Owned> + 'static,
    {
        self.0.collect_to(e)
    }
    pub fn collect<E>(self) -> Fold<E>
    where
        T: ToOwned,
        E: Extend<<T as ToOwned>::Owned> + Default + 'static,
    {
        self.0.collect()
    }
    pub fn collect_vec(self) -> Fold<Vec<<T as ToOwned>::Owned>>
    where
        T: ToOwned,
    {
        self.0.collect_vec()
    }

    pub fn subscribe(self, f: impl FnMut(&T) + 'static) -> Subscription {
        self.0.subscribe(f)
    }
    pub fn subscribe_to<O>(self, o: O) -> DynSubscriber<O>
    where
        for<'a> O: Observer<&'a T>,
    {
        self.0.subscribe_to(o).into_dyn()
    }
}

pub struct Tail<S>(Option<TailData<S>>);

impl<S: Observable + 'static> Tail<S> {
    pub(crate) fn new<U>(source: S, scope: &BindScope, f: impl FnOnce(&S::Item) -> U) -> (U, Self) {
        let state = TailState::new();
        let mut b = state.borrow_mut();
        let head = b
            .bindings
            .update(scope, &state, |bc| source.with(|value, _| f(value), bc));
        let tail = if b.bindings.is_empty() {
            Self(None)
        } else {
            drop(b);
            Self(Some(TailData { source, state }))
        };
        (head, tail)
    }
    pub fn empty() -> Self {
        Self(None)
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    pub fn fold<St: 'static>(
        self,
        initial_state: St,
        mut f: impl FnMut(&mut St, &S::Item) + 'static,
    ) -> Fold<St> {
        if let Some(this) = self.0 {
            let fold = this.subscribe_new(Some(initial_state), move |st, value, _bc| {
                if let Some(st) = st {
                    f(st, value);
                }
            });
            Fold::from_dyn(fold)
        } else {
            Fold::constant(initial_state)
        }
    }
    pub fn collect_to<E>(self, e: E) -> Fold<E>
    where
        S::Item: ToOwned,
        E: Extend<<S::Item as ToOwned>::Owned> + 'static,
    {
        self.fold(e, |e, x| e.extend(once(x.to_owned())))
    }
    pub fn collect<E>(self) -> Fold<E>
    where
        S::Item: ToOwned,
        E: Extend<<S::Item as ToOwned>::Owned> + Default + 'static,
    {
        self.collect_to(Default::default())
    }
    pub fn collect_vec(self) -> Fold<Vec<<S::Item as ToOwned>::Owned>>
    where
        S::Item: ToOwned,
    {
        self.collect()
    }

    pub fn subscribe(self, f: impl FnMut(&S::Item) + 'static) -> Subscription {
        self.subscribe_to(f).into_subscription()
    }
    pub fn subscribe_to<O>(self, o: O) -> impl Subscriber<St = O>
    where
        for<'a> O: Observer<&'a S::Item>,
    {
        if let Some(this) = self.0 {
            let s = this.subscribe_new(o, |o, value, _bc| o.next(value));
            MayConstantSubscriber::Subscriber(subscriber(s))
        } else {
            MayConstantSubscriber::Constant(RefCell::new(o))
        }
    }
}

struct TailData<S> {
    source: S,
    state: Rc<RefCell<TailState>>,
}
impl<S: Observable + 'static> TailData<S> {
    fn subscribe_new<St: 'static>(
        self,
        st: St,
        mut f: impl FnMut(&mut St, &S::Item, &mut ObsContext) + 'static,
    ) -> Rc<
        Subscribe<
            TailSubscriberState<St>,
            impl FnMut(&mut TailSubscriberState<St>, &mut ObsContext),
        >,
    > {
        let mut state = self.state.borrow_mut();
        let head_subscription = if !state.is_modified {
            Some(self.state.clone())
        } else {
            None
        };
        let st = TailSubscriberState {
            st,
            head_subscription,
        };
        let source = self.source;
        let bindings = mem::take(&mut state.bindings);
        let s = Subscribe::new_tail(
            st,
            move |st, bc| {
                source.with(|value, bc| f(&mut st.st, value, bc), bc);
                st.head_subscription = None;
            },
            state.is_modified,
            bindings,
        );
        if !state.is_modified {
            state.sink = Some(s.clone());
        }
        s
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

struct TailSubscriberState<St> {
    st: St,
    head_subscription: Option<Rc<RefCell<TailState>>>,
}
impl<St> SubscriberState for TailSubscriberState<St> {
    type St = St;
    fn borrow(&self) -> &Self::St {
        &self.st
    }
    fn borrow_mut(&mut self) -> &mut Self::St {
        &mut self.st
    }
}
impl<T> FoldState for TailSubscriberState<Option<T>> {
    type Output = T;
    fn finish(&mut self) -> Self::Output {
        self.st.finish()
    }
}
