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
    fn reset(&self) -> bool {
        let mut s = self.state.borrow_mut();
        let is_some = s.value.is_some();
        if is_some {
            s.value = None;
            s.bindings.clear();
        }
        is_some
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
            b.value = b.bindings.update(&rc_self, |ctx| Some(self.s.get(ctx)));
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
            self.reset();
        }
    }
}
impl<T: 'static> BindSink for Cached<T> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        if self.reset() {
            self.sinks.notify(ctx);
        }
    }
}
