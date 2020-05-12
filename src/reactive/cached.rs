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
    bindings: Vec<Binding>,
}
impl<T> Cached<T> {
    pub fn new(s: Re<T>) -> Self {
        Cached {
            s,
            sinks: BindSinks::new(),
            state: RefCell::new(CachedState {
                value: None,
                bindings: Vec::new(),
            }),
        }
    }
}
impl<T: 'static> DynReRefSource for Cached<T> {
    type Item = T;

    fn dyn_borrow<'a>(
        &'a self,
        rc_self: &Rc<dyn DynReRefSource<Item = Self::Item>>,
        ctx: &mut ReactiveContext,
    ) -> Ref<'a, Self::Item> {
        let rc_self = Self::downcast(rc_self);
        ctx.bind(rc_self.clone());
        let mut s = self.state.borrow();
        if s.value.is_none() {
            drop(s);
            {
                let mut s = self.state.borrow_mut();
                let mut ctx = ReactiveContext::new(&rc_self, &mut s.bindings);
                s.value = Some(self.s.get(&mut ctx));
            }
            s = self.state.borrow();
        }
        return Ref::map(Ref::Cell(s), |o| o.value.as_ref().unwrap());
    }

    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}
impl<T: 'static> BindSource for Cached<T> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
impl<T: 'static> BindSink for Cached<T> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        let mut s = self.state.borrow_mut();
        if s.value.is_some() {
            s.value = None;
            s.bindings.clear();
            self.sinks.notify_with(ctx);
        }
    }
}
