use crate::dynamic_obs::*;
use crate::*;
use std::{cell::RefCell, rc::Rc};

pub struct Hot<S> {
    source: S,
    bindings: RefCell<Bindings>,
}
pub trait HotReady: 'static {
    fn ready(self: Rc<Self>, scope: &BindScope);
}

impl<S> Hot<S>
where
    DynamicObs<Self>: HotReady,
{
    pub fn new(source: S) -> Rc<DynamicObs<Self>> {
        let rc = Rc::new(DynamicObs(Self {
            source,
            bindings: RefCell::new(Bindings::new()),
        }));
        let this = rc.clone();
        BindScope::with(|scope| this.ready(scope));
        rc
    }
}
impl<S> BindSink for DynamicObs<Hot<S>>
where
    Self: HotReady,
{
    fn notify(self: Rc<Self>, _scope: &NotifyScope) {
        schedule_bind(&self);
    }
}
impl<S> BindTask for DynamicObs<Hot<S>>
where
    Self: HotReady,
{
    fn run(self: Rc<Self>, scope: &BindScope) {
        self.ready(scope);
    }
}

impl<T: 'static + ?Sized> HotReady for DynamicObs<Hot<DynObs<T>>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.0
            .bindings
            .borrow_mut()
            .update(scope, &this, |bc| self.0.source.with(|_, _| {}, bc));
    }
}
impl<S: Observable> HotReady for DynamicObs<Hot<Obs<S>>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.0
            .bindings
            .borrow_mut()
            .update(scope, &this, |bc| self.0.source.with(|_, _| {}, bc));
    }
}

impl<S: Observable> Observable for Hot<S> {
    type Item = S::Item;
    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        bc: &mut BindContext,
    ) -> U {
        self.source.with(f, bc)
    }
}
