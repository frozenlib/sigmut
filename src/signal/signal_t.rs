use std::{any::Any, future::Future, rc::Rc, task::Poll};

use derive_ex::{derive_ex, Ex};
use futures::Stream;

use crate::{
    core::AsyncSignalContext, stream_from, subscribe, subscribe_with, Scheduler, SignalContext,
    StateRef, Subscription,
};

use super::{builder::SignalBuilder, scan_async::build_scan_async};

#[cfg(test)]
mod tests;

pub trait SignalNode: 'static {
    type Value: ?Sized + 'static;
    fn borrow<'a, 's: 'a>(
        self: Rc<Self>,
        inner: &'a Self,
        sc: &mut SignalContext<'s>,
    ) -> StateRef<'a, Self::Value>;
}

trait DynSignalNode {
    type Value: ?Sized + 'static;
    fn dyn_borrow<'a, 's: 'a>(
        self: Rc<Self>,
        inner: &'a dyn Any,
        sc: &mut SignalContext<'s>,
    ) -> StateRef<'a, Self::Value>;

    fn as_any(&self) -> &dyn Any;
}

impl<S: SignalNode + 'static> DynSignalNode for S {
    type Value = S::Value;

    fn dyn_borrow<'a, 's: 'a>(
        self: Rc<Self>,
        inner: &'a dyn Any,
        sc: &mut SignalContext<'s>,
    ) -> StateRef<'a, Self::Value> {
        self.borrow(inner.downcast_ref().unwrap(), sc)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive_ex(Clone)]
enum RawSignal<T: ?Sized + 'static> {
    StaticRef(&'static T),
    Node(Rc<dyn DynSignalNode<Value = T>>),
}

#[derive(Ex)]
#[derive_ex(Clone)]
pub struct Signal<T: ?Sized + 'static>(RawSignal<T>);

impl<T: ?Sized + 'static> Signal<T> {
    pub fn new(f: impl Fn(&mut SignalContext) -> T + 'static) -> Self
    where
        T: Sized,
    {
        SignalBuilder::new(f).build()
    }
    pub fn new_dedup(f: impl Fn(&mut SignalContext) -> T + 'static) -> Self
    where
        T: Sized + PartialEq,
    {
        SignalBuilder::new(f).dedup().build()
    }

    pub fn from_value(value: T) -> Self
    where
        T: Sized,
    {
        Self::from_value_map(value, |x| x)
    }
    pub fn from_value_map<U>(value: U, f: impl Fn(&U) -> &T + 'static) -> Self
    where
        U: 'static,
    {
        Self::from_node(Rc::new(ConstantNode { value, map: f }))
    }
    pub fn from_owned(owned: impl std::borrow::Borrow<T> + 'static) -> Self {
        Self::from_value_map(owned, |x| std::borrow::Borrow::borrow(x))
    }
    pub fn from_node(node: Rc<impl SignalNode<Value = T>>) -> Self {
        Signal(RawSignal::Node(node))
    }
    pub fn from_borrow<U>(
        this: U,
        borrow: impl for<'s, 'a> Fn(&'a U, &mut SignalContext<'s>, &'a &'s ()) -> StateRef<'a, T>
            + 'static,
    ) -> Self
    where
        U: 'static,
    {
        Self::from_node(Rc::new(FromBorrowNode { this, borrow }))
    }

    pub fn from_static_ref(value: &'static T) -> Self {
        Signal(RawSignal::StaticRef(value))
    }

    pub fn from_future(future: impl Future<Output = T> + 'static) -> Signal<Poll<T>>
    where
        T: Sized,
    {
        SignalBuilder::from_future_scan(Poll::Pending, future, |st, value| *st = Poll::Ready(value))
            .build()
    }
    pub fn from_stream(stream: impl Stream<Item = T> + 'static) -> Signal<Poll<T>>
    where
        T: Sized,
    {
        SignalBuilder::from_stream_scan_filter(Poll::Pending, stream, |st, value| {
            if let Some(value) = value {
                *st = Poll::Ready(value);
                true
            } else {
                false
            }
        })
        .build()
    }

    pub fn from_async<Fut>(f: impl Fn(AsyncSignalContext) -> Fut + 'static) -> Signal<Poll<T>>
    where
        Fut: Future<Output = T> + 'static,
        T: Sized,
    {
        build_scan_async(
            Poll::Pending,
            f,
            |st, poll| {
                if st.is_pending() && poll.is_pending() {
                    false
                } else {
                    *st = poll;
                    true
                }
            },
            |st| st,
        )
    }

    pub fn borrow<'a, 's: 'a>(&'a self, sc: &mut SignalContext<'s>) -> StateRef<'a, T> {
        match &self.0 {
            RawSignal::StaticRef(value) => StateRef::from(*value),
            RawSignal::Node(node) => node.clone().dyn_borrow(node.as_any(), sc),
        }
    }
    pub fn get(&self, sc: &mut SignalContext) -> <T as ToOwned>::Owned
    where
        T: ToOwned,
    {
        self.borrow(sc).into_owned()
    }

    pub fn map<U>(&self, f: impl Fn(&T) -> &U + 'static) -> Signal<U> {
        Signal::from_borrow(self.clone(), move |this, sc, _| {
            StateRef::map(this.borrow(sc), &f, sc)
        })
    }

    pub fn dedup(&self) -> Signal<T>
    where
        T: ToOwned,
        T: PartialEq,
    {
        let this = self.clone();
        SignalBuilder::from_scan_filter(None, move |st, sc| {
            let value = this.borrow(sc);
            if let Some(old) = st {
                if std::borrow::Borrow::borrow(&*old) == &*value {
                    return false;
                }
                value.clone_into(old);
            } else {
                *st = Some(value.into_owned());
            }
            true
        })
        .map(|st| std::borrow::Borrow::borrow(st.as_ref().unwrap()))
        .build()
    }

    pub fn subscribe(&self, mut f: impl FnMut(&T) + 'static) -> Subscription {
        let this = self.clone();
        subscribe(move |sc| f(&this.borrow(sc)))
    }
    pub fn subscribe_with(
        &self,
        mut f: impl FnMut(&T) + 'static,
        scheduler: &Scheduler,
    ) -> Subscription {
        let this = self.clone();
        subscribe_with(move |sc| f(&this.borrow(sc)), scheduler)
    }

    pub fn to_stream(&self) -> impl Stream<Item = T::Owned> + Unpin + 'static
    where
        T: ToOwned,
    {
        let this = self.clone();
        stream_from(move |sc| this.get(sc))
    }
    pub fn to_stream_map<U: 'static>(
        &self,
        f: impl Fn(&T) -> U + 'static,
    ) -> impl Stream<Item = U> + Unpin + 'static {
        let this = self.clone();
        stream_from(move |sc| f(&this.borrow(sc)))
    }
}
impl<T: ?Sized + 'static> ToSignal for Signal<T> {
    type Value = T;
    fn to_signal(&self) -> Signal<Self::Value> {
        self.clone()
    }
}

struct ConstantNode<St, M> {
    value: St,
    map: M,
}
impl<St, M, T> SignalNode for ConstantNode<St, M>
where
    St: 'static,
    M: Fn(&St) -> &T + 'static,
    T: ?Sized + 'static,
{
    type Value = T;

    fn borrow<'a, 's: 'a>(
        self: Rc<Self>,
        inner: &'a Self,
        sc: &mut SignalContext<'s>,
    ) -> StateRef<'a, Self::Value> {
        StateRef::map((&inner.value).into(), &inner.map, sc)
    }
}

struct FromBorrowNode<T, F> {
    this: T,
    borrow: F,
}
impl<T, F, O> SignalNode for FromBorrowNode<T, F>
where
    T: 'static,
    F: for<'a, 's> Fn(&'a T, &mut SignalContext<'s>, &'a &'s ()) -> StateRef<'a, O> + 'static,
    O: ?Sized + 'static,
{
    type Value = O;

    fn borrow<'a, 's: 'a>(
        self: Rc<Self>,
        inner: &'a Self,
        sc: &mut SignalContext<'s>,
    ) -> StateRef<'a, Self::Value> {
        (inner.borrow)(&inner.this, sc, &&())
    }
}

pub trait ToSignal {
    type Value: ?Sized + 'static;
    fn to_signal(&self) -> Signal<Self::Value>;
}
impl<T> ToSignal for &T
where
    T: ?Sized + ToSignal,
{
    type Value = T::Value;
    fn to_signal(&self) -> Signal<Self::Value> {
        (*self).to_signal()
    }
}
