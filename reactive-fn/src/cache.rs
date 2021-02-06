use super::*;
use std::{
    cell::{Ref, RefCell, RefMut},
    rc::Rc,
};
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
    pub fn borrow_mut(
        &self,
        f: impl FnOnce(&mut BindContext) -> T,
        cx: &mut BindContext,
    ) -> RefMut<T> {
        self.load(f, cx.scope());
        self.try_borrow_mut(cx).unwrap()
    }
    pub fn borrow(&self, f: impl FnOnce(&mut BindContext) -> T, cx: &mut BindContext) -> Ref<T> {
        self.load(f, cx.scope());
        self.try_borrow(cx).unwrap()
    }
    fn load(&self, f: impl FnOnce(&mut BindContext) -> T, scope: &BindScope) {
        if self.0.state.borrow().value.is_some() {
            return;
        }
        let mut b = self.0.state.borrow_mut();
        b.value = Some(b.bindings.update(scope, &self.0, f));
    }
    pub fn try_borrow_mut(&self, cx: &mut BindContext) -> Option<RefMut<T>> {
        cx.bind(self.0.clone());
        self.cache_mut()
    }
    pub fn try_borrow(&self, cx: &mut BindContext) -> Option<Ref<T>> {
        cx.bind(self.0.clone());
        self.cache()
    }
    pub fn cache_mut(&self) -> Option<RefMut<T>> {
        let r = RefMut::map(self.0.state.borrow_mut(), |x| &mut x.value);
        if r.is_some() {
            Some(RefMut::map(r, |x| x.as_mut().unwrap()))
        } else {
            None
        }
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
