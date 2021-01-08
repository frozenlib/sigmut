use std::{
    cell::{Ref, RefCell},
    rc::Rc,
};

use crate::*;
pub struct Cache<T>(Rc<CacheData<T>>);

struct CacheData<T> {
    sinks: BindSinks,
    state: RefCell<CacheState<T>>,
}

struct CacheState<T> {
    value: Option<T>,
    bindings: Bindings,
}

impl<T: 'static> Cache<T> {
    pub fn new() -> Self {
        Self(Rc::new(CacheData {
            sinks: BindSinks::new(),
            state: RefCell::new(CacheState {
                value: None,
                bindings: Bindings::new(),
            }),
        }))
    }

    pub fn is_cached(&self) -> bool {
        self.0.state.borrow().value.is_some()
    }
    pub fn borrow(&mut self, f: impl Fn(&BindContext) -> T, cx: &BindContext) -> Ref<T> {
        if let Some(r) = self.try_borrow(cx) {
            return r;
        }
        let mut b = self.0.state.borrow_mut();
        if b.value.is_none() {
            b.value = Some(b.bindings.update(cx.scope(), &self.0, f));
        }
        drop(b);
        self.try_borrow(cx).unwrap()
    }
    pub fn try_borrow(&self, cx: &BindContext) -> Option<Ref<T>> {
        cx.bind(self.0.clone());
        let r = Ref::map(self.0.state.borrow(), |x| &x.value);
        if r.is_some() {
            Some(Ref::map(r, |x| x.as_ref().unwrap()))
        } else {
            None
        }
    }
}
impl<T: 'static> BindSink for CacheData<T> {
    fn notify(self: Rc<Self>, scope: &NotifyScope) {
        self.state.borrow_mut().value.take();
        self.sinks.notify(scope);
    }
}
impl<T: 'static> BindSource for CacheData<T> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
