use super::*;
use crate::dynamic_obs::*;
use std::{
    cell::{Ref, RefCell},
    rc::Rc,
};

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
    fn notify(self: Rc<Self>, scope: &NotifyScope) {
        scope.defer_bind(self);
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

impl<S: Observable> HotReady for DynamicObs<Hot<Obs<S>>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.0
            .bindings
            .borrow_mut()
            .update(scope, &this, |cx| self.0.source.get(cx));
    }
}
impl<S: ObservableBorrow> HotReady for DynamicObs<Hot<ObsBorrow<S>>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.0.bindings.borrow_mut().update(scope, &this, |cx| {
            self.0.source.borrow(cx);
        });
    }
}

impl<T: 'static + ?Sized> HotReady for DynamicObs<Hot<DynObsRef<T>>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.0
            .bindings
            .borrow_mut()
            .update(scope, &this, |cx| self.0.source.with(|_, _| {}, cx));
    }
}
impl<S: ObservableRef> HotReady for DynamicObs<Hot<ObsRef<S>>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.0
            .bindings
            .borrow_mut()
            .update(scope, &this, |cx| self.0.source.with(|_, _| {}, cx));
    }
}

impl<S: Observable> Observable for Hot<S> {
    type Item = S::Item;
    fn get(&self, cx: &BindContext) -> Self::Item {
        self.source.get(cx)
    }
}
impl<S: ObservableBorrow> ObservableBorrow for Hot<S> {
    type Item = S::Item;
    fn borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.source.borrow(cx)
    }
}

impl<S: ObservableRef> ObservableRef for Hot<S> {
    type Item = S::Item;
    fn with<U>(&self, f: impl FnOnce(&Self::Item, &BindContext) -> U, cx: &BindContext) -> U {
        self.source.with(f, cx)
    }
}
