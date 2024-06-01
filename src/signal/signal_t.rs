use std::{
    any::{type_name, Any},
    fmt::Debug,
    future::Future,
    ptr,
    rc::Rc,
    task::Poll,
};

use derive_ex::{derive_ex, Ex};
use futures::Stream;

use crate::{
    core::AsyncSignalContext, effect, effect_with, stream_from, SignalContext, StateRef,
    Subscription, TaskKind,
};

use super::{builder::SignalBuilder, scan_async::build_scan_async};

#[cfg(test)]
mod tests;

mod keep;

pub trait SignalNode: 'static {
    type Value: ?Sized + 'static;
    fn borrow<'a, 's: 'a>(
        self: Rc<Self>,
        inner: &'a Self,
        sc: &mut SignalContext<'s>,
    ) -> StateRef<'a, Self::Value>;

    fn fmt_debug(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result
    where
        Self::Value: Debug;
}

trait DynSignalNode {
    type Value: ?Sized + 'static;
    fn dyn_borrow<'a, 's: 'a>(
        self: Rc<Self>,
        inner: &'a dyn Any,
        sc: &mut SignalContext<'s>,
    ) -> StateRef<'a, Self::Value>;

    fn dyn_fmt_debug(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result
    where
        Self::Value: Debug;

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
    fn dyn_fmt_debug(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result
    where
        Self::Value: Debug,
    {
        self.fmt_debug(f)
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
impl<T: ?Sized + 'static> RawSignal<T> {
    fn ptr_eq(this: &Self, other: &Self) -> bool {
        match (this, other) {
            (Self::StaticRef(a), Self::StaticRef(b)) => ptr::eq(a, b),
            (Self::Node(a), Self::Node(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// Similar to `Rc<dyn Fn() -> &T>`, but with added functionality to observe changes in the result.
///
/// Use the following methods to create an instance of `Signal`.
///
/// - Methods of `Signal`
/// - Methods of [`SignalBuilder`]
/// - [`ToSignal::to_signal`]
#[derive(Ex)]
#[derive_ex(Clone)]
pub struct Signal<T: ?Sized + 'static>(RawSignal<T>);

impl<T: ?Sized + 'static> Signal<T> {
    /// Create a new `Signal` from a function to get a value.
    ///
    /// The signal created by this function also sends a notification when `f` returns the same value as before.
    /// [`new_dedup`](Self::new_dedup) must be used to avoid sending a notification when `f` returns the same value as before.
    pub fn new(f: impl Fn(&mut SignalContext) -> T + 'static) -> Self
    where
        T: Sized,
    {
        SignalBuilder::new(f).build()
    }

    /// Creates a new `Signal` from a function to get a value, with deduplication.
    ///
    /// The signal created by this function does not send a notification when `f` returns the same value as before.
    ///
    /// Even if the value is not changed, a "value may have changed" notification is sent to the dependants, so the overhead cannot be zero.
    pub fn new_dedup(f: impl Fn(&mut SignalContext) -> T + 'static) -> Self
    where
        T: Sized + PartialEq,
    {
        SignalBuilder::new(f).dedup().build()
    }

    /// Create a new `Signal` that does not change from its value.
    pub fn from_value(value: T) -> Self
    where
        T: Sized,
    {
        Self::from_value_map(value, |x| x)
    }

    /// Create a new `Signal` that does not change from its value, with a mapping function.
    pub fn from_value_map<U>(value: U, f: impl Fn(&U) -> &T + 'static) -> Self
    where
        U: 'static,
    {
        Self::from_node(Rc::new(ConstantNode { value, map: f }))
    }

    /// Creates a new `Signal` from an owned value.
    pub fn from_owned(owned: impl std::borrow::Borrow<T> + 'static) -> Self {
        Self::from_value_map(owned, |x| std::borrow::Borrow::borrow(x))
    }

    /// Create a new `Signal` by specifying the internal implementation.
    pub fn from_node(node: Rc<impl SignalNode<Value = T>>) -> Self {
        Signal(RawSignal::Node(node))
    }

    /// Create a new `Signal` from a function to get [`StateRef`].
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

    /// Create a new `Signal` that does not change from a static reference.
    pub fn from_static_ref(value: &'static T) -> Self {
        Signal(RawSignal::StaticRef(value))
    }

    /// Create a new `Signal` from a [`Future`].
    pub fn from_future(future: impl Future<Output = T> + 'static) -> Signal<Poll<T>>
    where
        T: Sized,
    {
        SignalBuilder::from_future_scan(Poll::Pending, future, |st, value| *st = Poll::Ready(value))
            .build()
    }

    /// Create a new `Signal` from a [`Stream`].
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

    /// Create a `Signal` from an asynchronous function to get a value.
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

    /// Obtains a reference to the current value and adds a dependency on this `Signal` to the specified `SignalContext`.
    ///
    /// If the current value has not yet been calculated, it will be calculated.
    pub fn borrow<'a, 's: 'a>(&'a self, sc: &mut SignalContext<'s>) -> StateRef<'a, T> {
        match &self.0 {
            RawSignal::StaticRef(value) => StateRef::from(*value),
            RawSignal::Node(node) => node.clone().dyn_borrow(node.as_any(), sc),
        }
    }

    /// Gets the current value and adds a dependency on this `Signal` to the specified `SignalContext`.
    ///
    /// If the current value has not yet been calculated, it will be calculated.
    pub fn get(&self, sc: &mut SignalContext) -> <T as ToOwned>::Owned
    where
        T: ToOwned,
    {
        self.borrow(sc).into_owned()
    }

    /// Creates a new `Signal` whose references are transformed by the specified function.
    ///
    /// Using [`SignalBuilder::map`], you can create similar `Signal` more efficiently.
    pub fn map<U: ?Sized>(&self, f: impl Fn(&T) -> &U + 'static) -> Signal<U> {
        Signal::from_borrow(self.clone(), move |this, sc, _| {
            StateRef::map(this.borrow(sc), &f, sc)
        })
    }

    /// Create a `Signal` that does not send notifications to the dependants if the value does not change.
    ///
    /// Even if the value is not changed, a "value may have changed" notification is sent to the dependants, so the overhead cannot be zero.
    ///
    /// Using [`SignalBuilder::dedup`], you can create similar `Signal` more efficiently.
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

    /// Create a `Signal` that keeps a cache even if the subscriber does not exist.
    ///
    /// Normally, `Signal` discards the cache at the time [`Runtime::run_discards`](crate::core::Runtime::run_discards) is called if there are no subscribers.
    /// Signals created by this method do not discards the cache even if there are no subscribers.
    pub fn keep(&self) -> Signal<T> {
        keep::keep_node(self.clone())
    }

    /// Subscribe to the value of this signal.
    ///
    /// First, call the function with the current value, then call the function each time the value changes.
    ///
    /// The function is called when [`Runtime::run_tasks`](crate::core::Runtime::run_tasks) is called with `None` or `Some(TaskKind::default())`.
    ///
    /// When the `Subscription` returned by this function is dropped, the subscription is canceled.
    pub fn effect(&self, mut f: impl FnMut(&T) + 'static) -> Subscription {
        let this = self.clone();
        effect(move |sc| f(&this.borrow(sc)))
    }

    /// Subscribe to the value of this signal with specifying [`TaskKind`].
    ///
    /// First, call the function with the current value, then call the function each time the value changes.
    ///
    /// The function is called when [`Runtime::run_tasks`](crate::core::Runtime::run_tasks) is called with `None` or `Some(kind)`.
    ///
    /// When the `Subscription` returned by this function is dropped, the subscription is canceled.
    pub fn effect_with(&self, mut f: impl FnMut(&T) + 'static, kind: TaskKind) -> Subscription {
        let this = self.clone();
        effect_with(move |sc| f(&this.borrow(sc)), kind)
    }

    /// Create a [`Stream`] to subscribe to the value of this signal.
    pub fn to_stream(&self) -> impl Stream<Item = T::Owned> + Unpin + 'static
    where
        T: ToOwned,
    {
        let this = self.clone();
        stream_from(move |sc| this.get(sc))
    }

    /// Create a [`Stream`] that subscribes to the value of this signal by specifying a conversion function.
    pub fn to_stream_map<U: 'static>(
        &self,
        f: impl Fn(&T) -> U + 'static,
    ) -> impl Stream<Item = U> + Unpin + 'static {
        let this = self.clone();
        stream_from(move |sc| f(&this.borrow(sc)))
    }

    /// Returns true if two [`Signal`] instances are equal.
    ///
    /// Signal works like [`Rc`];
    /// just as [`Rc::ptr_eq`] allows you to check if the instance of Rc is the same,
    /// this function allows you to check if the instance of Signal is the same.
    pub fn ptr_eq(this: &Self, other: &Self) -> bool {
        RawSignal::ptr_eq(&this.0, &other.0)
    }
}
impl<T: 'static> Signal<Poll<T>> {
    /// Waits until the current value is `Ready` and returns that value.
    pub async fn get_async(&self, sc: &mut AsyncSignalContext) -> T
    where
        T: Clone,
    {
        sc.poll_fn(|sc| self.get(sc)).await
    }
}
impl<T: 'static + ?Sized + Debug> Debug for Signal<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            RawSignal::StaticRef(value) => value.fmt(f),
            RawSignal::Node(node) => node.dyn_fmt_debug(f),
        }
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

    fn fmt_debug(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result
    where
        Self::Value: Debug,
    {
        (self.map)(&self.value).fmt(f)
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

    fn fmt_debug(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result
    where
        Self::Value: Debug,
    {
        write!(f, "<borrow({})>", type_name::<T>())
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
