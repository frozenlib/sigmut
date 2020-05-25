use super::*;
use crate::bind::*;
use std::{cell::RefCell, rc::Rc};

pub struct Hot<S> {
    source: S,
    bindings: RefCell<Bindings>,
}
pub trait HotReady: 'static {
    fn ready(self: Rc<Self>, scope: &BindContextScope);
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
        BindContextScope::with(|scope| this.ready(scope));
        rc
    }
}
impl<S> BindSink for Hot<S>
where
    Self: HotReady,
{
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        ctx.spawn(Rc::downgrade(&self));
    }
}
impl<S> Task for Hot<S>
where
    Self: HotReady,
{
    fn run(self: Rc<Self>, scope: &BindContextScope) {
        self.ready(scope);
    }
}

impl<T: 'static> HotReady for Hot<Re<T>> {
    fn ready(self: Rc<Self>, scope: &BindContextScope) {
        let this = self.clone();
        self.bindings
            .borrow_mut()
            .update(scope, &this, |ctx| self.source.get(ctx));
    }
}
impl<T: 'static + ?Sized> HotReady for Hot<ReBorrow<T>> {
    fn ready(self: Rc<Self>, scope: &BindContextScope) {
        let this = self.clone();
        self.bindings
            .borrow_mut()
            .update(scope, &this, |ctx| self.source.borrow(ctx));
    }
}
impl<T: 'static + ?Sized> HotReady for Hot<ReRef<T>> {
    fn ready(self: Rc<Self>, scope: &BindContextScope) {
        let this = self.clone();
        self.bindings
            .borrow_mut()
            .update(scope, &this, |ctx| self.source.with(ctx, |_| {}));
    }
}

impl<T: 'static> DynRe for Hot<Re<T>> {
    type Item = T;
    fn dyn_get(&self, ctx: &mut BindContext) -> Self::Item {
        self.source.get(ctx)
    }
}
impl<T: 'static + ?Sized> DynReBorrow for Hot<ReBorrow<T>> {
    type Item = T;
    fn dyn_borrow(&self, ctx: &mut BindContext) -> Ref<Self::Item> {
        self.source.borrow(ctx)
    }
}

impl<T: 'static + ?Sized> DynReRef for Hot<ReRef<T>> {
    type Item = T;
    fn dyn_with(&self, ctx: &mut BindContext, f: &mut dyn FnMut(&Self::Item)) {
        self.source.with(ctx, f)
    }
}
