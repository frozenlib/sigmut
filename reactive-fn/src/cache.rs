use super::*;
use derive_ex::derive_ex;
use std::{
    cell::{Ref, RefCell},
    rc::Rc,
};

#[derive_ex(Default)]
#[default(Self::new())]
pub struct Cache<T>(Rc<CacheData<T>>);

struct CacheData<T> {
    sinks: BindSinks,
    state: RefCell<CacheState<T>>,
}

struct CacheState<T> {
    value: Option<T>,
    bindings: Bindings,
}

impl<T> Cache<T> {
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
    pub fn cache(&self) -> Option<Ref<T>> {
        let r = Ref::map(self.0.state.borrow(), |x| &x.value);
        if r.is_some() {
            Some(Ref::map(r, |x| x.as_ref().unwrap()))
        } else {
            None
        }
    }
}

impl<T: 'static> Cache<T> {
    pub fn borrow(&self, f: impl FnOnce(&mut BindContext) -> T, bc: &mut BindContext) -> Ref<T> {
        self.load(f, bc.scope());
        bc.bind(self.0.clone());
        self.cache().unwrap()
    }
    fn load(&self, f: impl FnOnce(&mut BindContext) -> T, scope: &BindScope) {
        if self.0.state.borrow().value.is_some() {
            return;
        }
        let mut b = self.0.state.borrow_mut();
        b.value = Some(b.bindings.update(scope, &self.0, f));
    }
}

impl<T: 'static> BindSink for CacheData<T> {
    fn notify(self: Rc<Self>, scope: &NotifyScope) {
        if self.state.borrow_mut().value.take().is_some() {
            self.sinks.notify(scope);
        }
    }
}
impl<T: 'static> BindSource for CacheData<T> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}
