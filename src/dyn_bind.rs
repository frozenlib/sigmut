use crate::*;
use std::any::Any;
use std::rc::Rc;

pub trait DynBind: 'static {
    type Item;

    fn dyn_get(self: Rc<Self>, ctx: &mut BindContext) -> Self::Item;
}
pub trait DynRefBind: 'static {
    type Item;

    fn dyn_borrow<'a>(&'a self, rc_this: &'a dyn Any, ctx: &mut BindContext)
        -> Ref<'a, Self::Item>;
    fn downcast(rc_this: &dyn Any) -> &Rc<Self>
    where
        Self: Sized,
    {
        rc_this.downcast_ref().unwrap()
    }
}

impl<B: Bind> DynBind for B {
    type Item = B::Item;
    fn dyn_get(self: Rc<Self>, ctx: &mut BindContext) -> Self::Item {
        self.get(ctx)
    }
}
impl<B: RefBind> DynRefBind for B {
    type Item = B::Item;

    fn dyn_borrow<'a>(
        &'a self,
        _rc_this: &'a dyn Any,
        ctx: &mut BindContext,
    ) -> Ref<'a, Self::Item> {
        self.borrow(ctx)
    }
}
pub type RcBind<T> = Rc<dyn DynBind<Item = T>>;
pub type RcRefBind<T> = Rc<dyn DynRefBind<Item = T>>;

impl<T: 'static> Bind for RcBind<T> {
    type Item = T;

    fn get(&self, ctx: &mut BindContext) -> Self::Item {
        self.clone().dyn_get(ctx)
    }
}
impl<T: 'static> RefBind for RcRefBind<T> {
    type Item = T;

    fn borrow(&self, ctx: &mut BindContext) -> Ref<Self::Item> {
        self.dyn_borrow(self, ctx)
    }
}
