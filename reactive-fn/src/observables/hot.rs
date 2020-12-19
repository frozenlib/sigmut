use super::*;
use crate::{bind::*, BindTask};
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
    Self: HotReady,
{
    pub fn new(source: S) -> Rc<Self> {
        let rc = Rc::new(Self {
            source,
            bindings: RefCell::new(Bindings::new()),
        });
        let this = rc.clone();
        BindScope::with(|scope| this.ready(scope));
        rc
    }
}
impl<S> BindSink for Hot<S>
where
    Self: HotReady,
{
    fn notify(self: Rc<Self>, scope: &NotifyScope) {
        scope.bind_defer(self);
    }
}
impl<S> BindTask for Hot<S>
where
    Self: HotReady,
{
    fn run(self: Rc<Self>, scope: &BindScope) {
        self.ready(scope);
    }
}

impl<T: 'static> HotReady for Hot<DynObs<T>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.bindings
            .borrow_mut()
            .update(scope, &this, |cx| self.source.get(cx));
    }
}
impl<S: Observable> HotReady for Hot<ReOps<S>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.bindings
            .borrow_mut()
            .update(scope, &this, |cx| self.source.get(cx));
    }
}
impl<T: 'static + ?Sized> HotReady for Hot<DynObsBorrow<T>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.bindings.borrow_mut().update(scope, &this, |cx| {
            self.source.borrow(cx);
        });
    }
}
impl<S: ObservableBorrow> HotReady for Hot<ReBorrowOps<S>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.bindings.borrow_mut().update(scope, &this, |cx| {
            self.source.borrow(cx);
        });
    }
}

impl<T: 'static + ?Sized> HotReady for Hot<ReRef<T>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.bindings
            .borrow_mut()
            .update(scope, &this, |cx| self.source.with(|_, _| {}, cx));
    }
}
impl<S: ObservableRef> HotReady for Hot<ReRefOps<S>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.bindings
            .borrow_mut()
            .update(scope, &this, |cx| self.source.with(|_, _| {}, cx));
    }
}

impl<S: Observable> Observable for Rc<Hot<ReOps<S>>> {
    type Item = S::Item;
    fn get(&self, cx: &BindContext) -> Self::Item {
        self.source.get(cx)
    }
}
impl<S: ObservableBorrow> ObservableBorrow for Rc<Hot<ReBorrowOps<S>>> {
    type Item = S::Item;
    fn borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.source.borrow(cx)
    }
}
impl<S: ObservableRef> ObservableRef for Rc<Hot<ReRefOps<S>>> {
    type Item = S::Item;
    fn with<U>(&self, f: impl FnOnce(&Self::Item, &BindContext) -> U, cx: &BindContext) -> U {
        self.source.with(f, cx)
    }
}

impl<S: Observable> DynamicObservable for Hot<ReOps<S>> {
    type Item = S::Item;
    fn dyn_get(&self, cx: &BindContext) -> Self::Item {
        self.source.get(cx)
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = Self::Item>> {
        self
    }
}

impl<S: Observable> DynamicObservableRef for Hot<ReOps<S>> {
    type Item = S::Item;
    fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
        f(&self.source.get(cx), cx)
    }
}
impl<S: ObservableBorrow> DynamicObservableBorrow for Hot<ReBorrowOps<S>> {
    type Item = S::Item;
    fn dyn_borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.source.borrow(cx)
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = Self::Item>> {
        self
    }
}
impl<S: ObservableBorrow> DynamicObservableRef for Hot<ReBorrowOps<S>> {
    type Item = S::Item;
    fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
        f(&self.source.borrow(cx), cx)
    }
}

impl<S: ObservableRef> DynamicObservableRef for Hot<ReRefOps<S>> {
    type Item = S::Item;
    fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
        self.source.with(f, cx)
    }
}
