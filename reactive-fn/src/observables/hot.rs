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
        scope.defer_bind(self);
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
impl<S: Observable> HotReady for Hot<Obs<S>> {
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
impl<S: ObservableBorrow> HotReady for Hot<ObsBorrow<S>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.bindings.borrow_mut().update(scope, &this, |cx| {
            self.source.borrow(cx);
        });
    }
}

impl<T: 'static + ?Sized> HotReady for Hot<DynObsRef<T>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.bindings
            .borrow_mut()
            .update(scope, &this, |cx| self.source.with(|_, _| {}, cx));
    }
}
impl<S: ObservableRef> HotReady for Hot<ObsRef<S>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.bindings
            .borrow_mut()
            .update(scope, &this, |cx| self.source.with(|_, _| {}, cx));
    }
}

impl<S: Observable> Observable for Rc<Hot<Obs<S>>> {
    type Item = S::Item;
    fn get(&self, cx: &BindContext) -> Self::Item {
        self.source.get(cx)
    }
}
impl<S: ObservableBorrow> ObservableBorrow for Rc<Hot<ObsBorrow<S>>> {
    type Item = S::Item;
    fn borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.source.borrow(cx)
    }
}
impl<S: ObservableRef> ObservableRef for Rc<Hot<ObsRef<S>>> {
    type Item = S::Item;
    fn with<U>(&self, f: impl FnOnce(&Self::Item, &BindContext) -> U, cx: &BindContext) -> U {
        self.source.with(f, cx)
    }
}

impl<S: Observable> DynamicObservable for Hot<Obs<S>> {
    type Item = S::Item;
    fn dyn_get(&self, cx: &BindContext) -> Self::Item {
        self.source.get(cx)
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = Self::Item>> {
        self
    }
}
impl<S: Observable> DynamicObservableRef for Hot<Obs<S>> {
    type Item = S::Item;
    fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
        f(&self.source.get(cx), cx)
    }

    fn copied(self: Rc<Self>) -> Rc<dyn DynamicObservable<Item = Self::Item>>
    where
        Self::Item: Copy,
    {
        self
    }
}
impl<S: ObservableBorrow> DynamicObservable for Hot<ObsBorrow<S>>
where
    S::Item: Copy,
{
    type Item = S::Item;
    fn dyn_get(&self, cx: &BindContext) -> Self::Item {
        *self.dyn_borrow(cx)
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = Self::Item>> {
        self
    }
}
impl<S: ObservableBorrow> DynamicObservableBorrow for Hot<ObsBorrow<S>> {
    type Item = S::Item;
    fn dyn_borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.source.borrow(cx)
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = Self::Item>> {
        self
    }
}
impl<S: ObservableBorrow> DynamicObservableRef for Hot<ObsBorrow<S>> {
    type Item = S::Item;
    fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
        f(&self.source.borrow(cx), cx)
    }
    fn copied(self: Rc<Self>) -> Rc<dyn DynamicObservable<Item = Self::Item>>
    where
        Self::Item: Copy,
    {
        self
    }
}

impl<S: ObservableRef> DynamicObservable for Hot<ObsRef<S>>
where
    S::Item: Copy,
{
    type Item = S::Item;
    fn dyn_get(&self, cx: &BindContext) -> Self::Item {
        let mut result = None;
        self.dyn_with(&mut |value, _| result = Some(*value), cx);
        result.unwrap()
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = Self::Item>> {
        self
    }
}
impl<S: ObservableRef> DynamicObservableRef for Hot<ObsRef<S>> {
    type Item = S::Item;
    fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
        self.source.with(f, cx)
    }

    fn copied(self: Rc<Self>) -> Rc<dyn DynamicObservable<Item = Self::Item>>
    where
        Self::Item: Copy,
    {
        self
    }
}
