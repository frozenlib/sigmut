use crate::*;
use std::any::Any;
use std::rc::Rc;

pub trait DynBind: 'static {
    type Item;

    fn dyn_bind(self: Rc<Self>, ctx: &mut BindContext) -> Self::Item;
}
pub trait DynRefBind: 'static {
    type Item;

    fn dyn_bind<'a>(&'a self, rc_this: &'a dyn Any, ctx: &mut BindContext) -> Ref<'a, Self::Item>;
    fn downcast(rc_this: &dyn Any) -> &Rc<Self>
    where
        Self: Sized + 'static,
    {
        rc_this.downcast_ref().unwrap()
    }
}

impl<B: Bind> DynBind for B {
    type Item = B::Item;
    fn dyn_bind(self: Rc<Self>, ctx: &mut BindContext) -> Self::Item {
        self.bind(ctx)
    }
}
impl<B: RefBind> DynRefBind for B {
    type Item = B::Item;

    fn dyn_bind<'a>(&'a self, _rc_this: &'a dyn Any, ctx: &mut BindContext) -> Ref<'a, Self::Item> {
        self.bind(ctx)
    }
}
pub type RcBind<T> = Rc<dyn DynBind<Item = T>>;
pub type RcRefBind<T> = Rc<dyn DynRefBind<Item = T>>;

impl<T: 'static> Bind for RcBind<T> {
    type Item = T;

    fn bind(&self, ctx: &mut BindContext) -> Self::Item {
        self.clone().dyn_bind(ctx)
    }
}
impl<T: 'static> RefBind for RcRefBind<T> {
    type Item = T;

    fn bind(&self, ctx: &mut BindContext) -> Ref<Self::Item> {
        self.dyn_bind(self, ctx)
    }
}
