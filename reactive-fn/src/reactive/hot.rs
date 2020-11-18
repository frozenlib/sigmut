use super::*;
use crate::bind::*;
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
        scope.spawn(self);
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

impl<T: 'static> HotReady for Hot<Re<T>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.bindings
            .borrow_mut()
            .update(scope, &this, |ctx| self.source.get(ctx));
    }
}
impl<S: Reactive> HotReady for Hot<ReOps<S>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.bindings
            .borrow_mut()
            .update(scope, &this, |ctx| self.source.get(ctx));
    }
}
impl<T: 'static + ?Sized> HotReady for Hot<ReBorrow<T>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.bindings.borrow_mut().update(scope, &this, |ctx| {
            self.source.borrow(ctx);
        });
    }
}
impl<S: ReactiveBorrow> HotReady for Hot<ReBorrowOps<S>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.bindings.borrow_mut().update(scope, &this, |ctx| {
            self.source.borrow(ctx);
        });
    }
}

impl<T: 'static + ?Sized> HotReady for Hot<ReRef<T>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.bindings
            .borrow_mut()
            .update(scope, &this, |ctx| self.source.with(|_, _| {}, ctx));
    }
}
impl<S: ReactiveRef> HotReady for Hot<ReRefOps<S>> {
    fn ready(self: Rc<Self>, scope: &BindScope) {
        let this = self.clone();
        self.bindings
            .borrow_mut()
            .update(scope, &this, |ctx| self.source.with(|_, _| {}, ctx));
    }
}

impl<S: Reactive> Reactive for Rc<Hot<ReOps<S>>> {
    type Item = S::Item;
    fn get(&self, ctx: &BindContext) -> Self::Item {
        self.source.get(ctx)
    }
}
impl<S: ReactiveBorrow> ReactiveBorrow for Rc<Hot<ReBorrowOps<S>>> {
    type Item = S::Item;
    fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.source.borrow(ctx)
    }
}
impl<S: ReactiveRef> ReactiveRef for Rc<Hot<ReRefOps<S>>> {
    type Item = S::Item;
    fn with<U>(&self, f: impl FnOnce(&Self::Item, &BindContext) -> U, ctx: &BindContext) -> U {
        self.source.with(f, ctx)
    }
}

impl<S: Reactive> DynamicReactive for Hot<ReOps<S>> {
    type Item = S::Item;
    fn dyn_get(&self, ctx: &BindContext) -> Self::Item {
        self.source.get(ctx)
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicReactiveRef<Item = Self::Item>> {
        self
    }
}

impl<S: Reactive> DynamicReactiveRef for Hot<ReOps<S>> {
    type Item = S::Item;
    fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), ctx: &BindContext) {
        f(&self.source.get(ctx), ctx)
    }
}
impl<S: ReactiveBorrow> DynamicReactiveBorrow for Hot<ReBorrowOps<S>> {
    type Item = S::Item;
    fn dyn_borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.source.borrow(ctx)
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicReactiveRef<Item = Self::Item>> {
        self
    }
}
impl<S: ReactiveBorrow> DynamicReactiveRef for Hot<ReBorrowOps<S>> {
    type Item = S::Item;
    fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), ctx: &BindContext) {
        f(&self.source.borrow(ctx), ctx)
    }
}

impl<S: ReactiveRef> DynamicReactiveRef for Hot<ReRefOps<S>> {
    type Item = S::Item;
    fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), ctx: &BindContext) {
        self.source.with(f, ctx)
    }
}
