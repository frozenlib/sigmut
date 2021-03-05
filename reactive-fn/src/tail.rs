use super::*;
use crate::bind::*;
use std::{cell::RefCell, mem, rc::Rc};

pub struct DynTail<T: ?Sized + 'static>(Tail<DynObs<T>>);

impl<T: ?Sized + 'static> DynTail<T> {
    pub(super) fn new<U>(
        source: DynObs<T>,
        scope: &BindScope,
        f: impl FnOnce(&T) -> U,
    ) -> (U, Self) {
        let (head, tail) = Tail::new(source, scope, f);
        (head, Self(tail))
    }
    pub fn empty() -> Self {
        Self(Tail::empty())
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    // pub fn subscribe(self, f: impl FnMut(&T) + 'static) -> Subscription {
    //     self.fold(f, move |mut f, x| {
    //         f(x);
    //         f
    //     })
    //     .into()
    // }
    // pub fn subscribe_to<O>(self, o: O) -> DynSubscriber<O>
    // where
    //     for<'a> O: Observer<&'a T>,
    // {
    //     self.0.subscribe_to(o).into_dyn()
    // }
    // pub fn fold<St: 'static>(
    //     self,
    //     initial_state: St,
    //     f: impl Fn(St, &T) -> St + 'static,
    // ) -> Fold<St> {
    //     self.0.fold(initial_state, f)
    // }
    // pub fn collect_to<E: for<'a> Extend<&'a T> + 'static>(self, e: E) -> Fold<E> {
    //     self.0.collect_to(e)
    // }
    // pub fn collect<E: for<'a> Extend<&'a T> + Default + 'static>(self) -> Fold<E> {
    //     self.0.collect()
    // }
    // pub fn collect_vec(self) -> Fold<Vec<T>>
    // where
    //     T: Copy,
    // {
    //     self.0.collect_vec()
    // }
}

pub struct Tail<S>(Option<TailData<S>>);

impl<S: Observable> Tail<S> {
    pub(crate) fn new<U>(source: S, scope: &BindScope, f: impl FnOnce(&S::Item) -> U) -> (U, Self) {
        let state = TailState::new();
        let mut b = state.borrow_mut();
        let head = b
            .bindings
            .update(scope, &state, |cx| source.with(|value, _| f(value), cx));
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

    // pub fn subscribe(self, f: impl FnMut(&S::Item) + 'static) -> Subscription {
    //     if let Some(this) = self.0 {
    //         let mut state = this.state.borrow_mut();
    //         let subscription = if state.is_modified {
    //             Some(this.state.clone())
    //         } else {
    //             None
    //         };
    //         let st = (f, subscription);
    //         let source = this.source;
    //         let bindings = mem::replace(&mut state.bindings, Bindings::new());
    //         let s = Subscribe::new_tail(
    //             st,
    //             move |st, cx| {
    //                 source.with(|value, _cx| (st.0)(value), cx);
    //                 st.1 = None;
    //             },
    //             !state.is_modified,
    //             bindings,
    //         );
    //         if !state.is_modified {
    //             state.sink = Some(s.clone());
    //         }
    //         subscriber(s).into_subscription()
    //     } else {
    //         Subscription::empty()
    //     }
    // }
    pub fn subscribe_to<O>(self, o: O) -> impl Subscriber<St = O>
    where
        for<'a> O: Observer<&'a S::Item>,
    {
        if let Some(this) = self.0 {
            let mut state = this.state.borrow_mut();
            let head_subscription = if state.is_modified {
                Some(this.state.clone())
            } else {
                None
            };
            let st = TailSubscriberState {
                st: o,
                head_subscription,
            };
            let source = this.source;
            let bindings = mem::replace(&mut state.bindings, Bindings::new());
            let s = Subscribe::new_tail(
                st,
                move |st, cx| {
                    source.with(|value, _cx| st.st.next(value), cx);
                    st.head_subscription = None;
                },
                !state.is_modified,
                bindings,
            );
            if !state.is_modified {
                state.sink = Some(s.clone());
            }
            MayConstantSubscriber::Subscriber(subscriber(s))
        } else {
            MayConstantSubscriber::Constant(RefCell::new(o))
        }
    }

    // pub fn fold<St: 'static>(
    //     self,
    //     initial_state: St,
    //     f: impl Fn(St, &S::Item) -> St + 'static,
    // ) -> Fold<St> {
    //     if let Some(this) = self.0 {
    //         let source = this.source;
    //         let fold = TailState::connect(this.state, initial_state, |s| {
    //             FoldBy::new_with_state(
    //                 s,
    //                 TailFoldByOp(fold_op(move |st, cx| {
    //                     source.with(|value, _| f(st, value), cx)
    //                 })),
    //             )
    //         });
    //         Fold::new(fold)
    //     } else {
    //         Fold::constant(initial_state)
    //     }
    // }
    // pub fn collect_to<E: for<'a> Extend<&'a S::Item> + 'static>(self, e: E) -> Fold<E> {
    //     self.fold(e, |mut e, x| {
    //         e.extend(once(x));
    //         e
    //     })
    // }
    // pub fn collect<E: for<'a> Extend<&'a S::Item> + Default + 'static>(self) -> Fold<E> {
    //     self.collect_to(Default::default())
    // }
    // pub fn collect_vec(self) -> Fold<Vec<S::Item>>
    // where
    //     S::Item: Copy,
    // {
    //     self.collect()
    // }
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
    // fn connect<U: BindSink, St>(
    //     this: Rc<RefCell<Self>>,
    //     initial_state: St,
    //     f: impl FnOnce(ScanState<(St, Option<Rc<RefCell<TailState>>>), St>) -> Rc<U>,
    // ) -> Rc<U> {
    //     let mut b = this.borrow_mut();
    //     let s = if b.is_modified {
    //         ScanState::Unloaded(initial_state)
    //     } else {
    //         ScanState::Loaded((initial_state, Some(this.clone())))
    //     };
    //     let tail = f(s);
    //     if !b.is_modified {
    //         b.sink = Some(tail.clone());
    //     }
    //     tail
    // }
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

impl BindSink for RefCell<TailState> {
    fn notify(self: Rc<Self>, scope: &NotifyScope) {
        let mut b = self.borrow_mut();
        b.is_modified = true;
        if let Some(sink) = b.sink.take() {
            sink.notify(scope);
        }
    }
}
// struct TailFoldByOp<Op>(Op);

// impl<Op: FoldByOp> FoldByOp for TailFoldByOp<Op> {
//     type LoadSt = (Op::LoadSt, Option<Rc<RefCell<TailState>>>);
//     type UnloadSt = Op::UnloadSt;
//     type Value = Op::Value;

//     fn load(&mut self, state: Self::UnloadSt, cx: &mut BindContext) -> Self::LoadSt {
//         (self.0.load(state, cx), None)
//     }

//     fn unload(&mut self, state: Self::LoadSt) -> Self::UnloadSt {
//         self.0.unload(state.0)
//     }
//     fn get(&self, state: Self::LoadSt) -> Self::Value {
//         self.0.get(state.0)
//     }
// }
// impl<Op: AsObserver<O>, O> AsObserver<O> for TailFoldByOp<Op> {
//     fn as_observer(&self) -> &O {
//         self.0.as_observer()
//     }
//     fn as_observer_mut(&mut self) -> &mut O {
//         self.0.as_observer_mut()
//     }
// }
