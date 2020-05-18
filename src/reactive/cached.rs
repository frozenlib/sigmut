use crate::bind::*;
use crate::reactive::*;
use std::{any::Any, cell::RefCell, rc::Rc};

pub struct Cached<T: 'static> {
    s: Re<T>,
    sinks: BindSinks,
    state: RefCell<CachedState<T>>,
}

struct CachedState<T> {
    value: Option<T>,
    bindings: Bindings,
}
impl<T> Cached<T> {
    pub fn new(s: Re<T>) -> Self {
        Cached {
            s,
            sinks: BindSinks::new(),
            state: RefCell::new(CachedState {
                value: None,
                bindings: Bindings::new(),
            }),
        }
    }
}
impl<T: 'static> DynReBorrowSource for Cached<T> {
    type Item = T;

    fn dyn_borrow(
        &self,
        rc_self: &Rc<dyn DynReBorrowSource<Item = Self::Item>>,
        ctx: &mut BindContext,
    ) -> Ref<Self::Item> {
        let rc_self = Self::downcast(rc_self);
        ctx.bind(rc_self.clone());
        let mut s = self.state.borrow();
        if s.value.is_none() {
            drop(s);
            let mut b = self.state.borrow_mut();
            b.value = b
                .bindings
                .update(ctx, &rc_self, |ctx| Some(self.s.get(ctx)));
            drop(b);
            s = self.state.borrow();
        }
        return Ref::map(s, |s| s.value.as_ref().unwrap());
    }

    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}
impl<T: 'static> BindSource for Cached<T> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
    fn detach_sink(&self, idx: usize, sink: &std::rc::Weak<dyn BindSink>) {
        self.sinks().detach(idx, sink);
        if self.sinks().is_empty() {
            let mut s = self.state.borrow_mut();
            s.bindings.clear();
            s.value = None;
        }
    }
}
impl<T: 'static> BindSink for Cached<T> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        let mut s = self.state.borrow_mut();
        if s.value.take().is_some() {
            drop(s);
            self.sinks.notify(ctx);
        }
    }
}
