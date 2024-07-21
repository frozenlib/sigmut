use std::{
    any::Any,
    mem::take,
    rc::{Rc, Weak},
};

use crate::utils::downcast;

/// Objects to continue to subscribe to while the instance is in existence.
#[derive(Default)]
#[must_use]
pub struct Subscription(RawSubscription);

impl Subscription {
    /// Creates a `Subscription` that subscribes to nothing.
    pub fn empty() -> Self {
        Subscription(RawSubscription::Empty)
    }
    /// Creates a `Subscription` with a function that is called upon unsubscription.
    pub fn from_fn(f: impl FnOnce() + 'static) -> Self {
        Subscription(RawSubscription::Fn(match downcast(f) {
            Ok(f) => f,
            Err(f) => Box::new(f),
        }))
    }
    /// Creates a `Subscription` with an `Rc` that will be dropped upon unsubscription.
    pub fn from_rc(rc: Rc<dyn Any>) -> Self {
        Subscription(RawSubscription::Rc(rc))
    }
    /// Creates a `Subscription` with an `Rc` and a function that is called upon unsubscription.
    ///
    /// If `unsubscribe` is ZST, no heap allocation occurs.
    pub fn from_rc_fn<T: 'static>(
        this: Rc<T>,
        unsubscribe: impl Fn(Rc<T>) + Copy + 'static,
    ) -> Self {
        Subscription(RawSubscription::RcFn {
            this,
            unsubscribe: Box::new(move |this| unsubscribe(this.downcast().unwrap())),
        })
    }

    /// Creates a `Subscription` with a `Weak` and a function that is called upon unsubscription.
    ///
    /// If `unsubscribe` is ZST, no heap allocation occurs.
    pub fn from_weak_fn<T: 'static>(
        this: Weak<T>,
        unsubscribe: impl Fn(Rc<T>) + Copy + 'static,
    ) -> Self {
        Subscription(RawSubscription::WeakFn {
            this,
            unsubscribe: Box::new(move |this| {
                if let Some(this) = this.upgrade() {
                    unsubscribe(this.downcast().unwrap())
                }
            }),
        })
    }
}
impl Drop for Subscription {
    fn drop(&mut self) {
        match take(&mut self.0) {
            RawSubscription::Empty => {}
            RawSubscription::Fn(f) => f(),
            RawSubscription::Rc(_) => {}
            RawSubscription::RcFn { this, unsubscribe } => unsubscribe(this),
            RawSubscription::WeakFn { this, unsubscribe } => unsubscribe(this),
        }
    }
}

#[derive(Default)]
enum RawSubscription {
    #[default]
    Empty,
    Fn(Box<dyn FnOnce() + 'static>),
    Rc(#[allow(unused)] Rc<dyn Any>),
    RcFn {
        this: Rc<dyn Any>,
        unsubscribe: Box<dyn Fn(Rc<dyn Any>)>,
    },
    WeakFn {
        this: Weak<dyn Any>,
        unsubscribe: Box<dyn Fn(Weak<dyn Any>)>,
    },
}
