use std::{
    any::Any,
    mem::take,
    rc::{Rc, Weak},
};

#[derive(Default)]
#[must_use]
pub struct Subscription(RawSubscription);

impl Subscription {
    pub fn empty() -> Self {
        Subscription(RawSubscription::Empty)
    }
    pub fn from_fn(f: impl FnOnce() + 'static) -> Self {
        Subscription(RawSubscription::Fn(Box::new(f)))
    }
    pub fn from_rc(rc: Rc<dyn Any>) -> Self {
        Subscription(RawSubscription::Rc(rc))
    }
    pub fn from_rc_fn<T: 'static>(
        this: Rc<T>,
        unsubscribe: impl Fn(Rc<T>) + Copy + 'static,
    ) -> Self {
        Subscription(RawSubscription::RcFn {
            this,
            unsubscribe: Box::new(move |this| unsubscribe(this.downcast().unwrap())),
        })
    }
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
