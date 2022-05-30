use crate::{
    core::{BindScope, BindSink, Bindings, NotifyScope},
    utils::cast_or_convert,
    BindContext,
};
use std::{cell::RefCell, rc::Rc};

pub struct BindContextBuilder(Rc<Data>);

struct Data {
    bindings: RefCell<Bindings>,
    on_notify: Box<dyn Fn(&NotifyScope)>,
}

impl BindContextBuilder {
    pub fn new(on_notify: impl Fn(&NotifyScope) + 'static) -> Self {
        Self::new_raw(cast_or_convert(on_notify, Box::new))
    }
    fn new_raw(on_notify: Box<dyn Fn(&NotifyScope)>) -> Self {
        Self(Rc::new(Data {
            bindings: RefCell::new(Bindings::new()),
            on_notify,
        }))
    }

    pub fn with<T>(&self, f: impl FnOnce(&mut BindContext) -> T, scope: &BindScope) -> T {
        self.0.bindings.borrow_mut().update(scope, &self.0, f)
    }
    pub fn with_new_scope<T>(&self, f: impl FnOnce(&mut BindContext) -> T) -> T {
        BindScope::with(|scope| self.with(f, scope))
    }
}
impl BindSink for Data {
    fn notify(self: Rc<Self>, scope: &NotifyScope) {
        (self.on_notify)(scope);
    }
}
