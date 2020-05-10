use crate::*;
use std::any::Any;
use std::rc::Rc;

pub trait DynBind: 'static {
    type Item;

    fn dyn_get(self: Rc<Self>, ctx: &mut ReactiveContext) -> Self::Item;
}
pub trait DynRefBind: 'static {
    type Item;

    fn dyn_borrow<'a>(
        &'a self,
        rc_this: &'a dyn Any,
        ctx: &mut ReactiveContext,
    ) -> Ref<'a, Self::Item>;
    fn downcast(rc_this: &dyn Any) -> &Rc<Self>
    where
        Self: Sized,
    {
        rc_this.downcast_ref().unwrap()
    }
}

impl<B: Reactive> DynBind for B {
    type Item = B::Item;
    fn dyn_get(self: Rc<Self>, ctx: &mut ReactiveContext) -> Self::Item {
        self.get(ctx)
    }
}
impl<B: ReactiveRef> DynRefBind for B {
    type Item = B::Item;

    fn dyn_borrow<'a>(
        &'a self,
        _rc_this: &'a dyn Any,
        ctx: &mut ReactiveContext,
    ) -> Ref<'a, Self::Item> {
        self.borrow(ctx)
    }
}
pub type RcBind<T> = Rc<dyn DynBind<Item = T>>;
pub type RcRefBind<T> = Rc<dyn DynRefBind<Item = T>>;

impl<T: 'static> Reactive for RcBind<T> {
    type Item = T;

    fn get(&self, ctx: &mut ReactiveContext) -> Self::Item {
        self.clone().dyn_get(ctx)
    }
}
impl<T: 'static> ReactiveRef for RcRefBind<T> {
    type Item = T;

    fn borrow(&self, ctx: &mut ReactiveContext) -> Ref<Self::Item> {
        self.dyn_borrow(self, ctx)
    }
}
