use super::*;
use std::cell::Ref;

pub trait Reactive: 'static {
    type Item;
    fn get(&self, ctx: &BindContext) -> Self::Item;

    fn into_dyn(self) -> Re<Self::Item>
    where
        Self: Sized,
    {
        struct IntoDyn<S>(S);
        impl<S: Reactive> DynRe for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_get(&self, ctx: &BindContext) -> Self::Item {
                self.0.get(ctx)
            }

            fn to_re_ref(self: Rc<Self>) -> ReRef<Self::Item> {
                ReRef(ReRefData::Dyn(self as Rc<dyn DynReRef<Item = Self::Item>>))
            }
        }
        impl<S: Reactive> DynReRef for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_with(&self, ctx: &BindContext, f: &mut dyn FnMut(&BindContext, &Self::Item)) {
                f(ctx, &self.0.get(ctx))
            }
        }
        Re::from_dyn(IntoDyn(self))
    }
}

pub trait ReactiveBorrow: 'static {
    type Item: ?Sized;
    fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item>;

    fn into_dyn(self) -> ReBorrow<Self::Item>
    where
        Self: Sized,
    {
        struct IntoDyn<S>(S);
        impl<S: ReactiveBorrow> DynReBorrow for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item> {
                self.0.borrow(ctx)
            }
        }
        impl<S: ReactiveBorrow> DynReRef for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_with(&self, ctx: &BindContext, f: &mut dyn FnMut(&BindContext, &Self::Item)) {
                f(ctx, &self.0.borrow(ctx))
            }
        }
        ReBorrow::from_dyn(IntoDyn(self))
    }
}

pub trait ReactiveRef: 'static {
    type Item: ?Sized;
    fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &Self::Item) -> U) -> U;

    fn into_dyn(self) -> ReRef<Self::Item>
    where
        Self: Sized,
    {
        struct IntoDyn<S>(S);
        impl<S: ReactiveRef> DynReRef for IntoDyn<S> {
            type Item = S::Item;
            fn dyn_with(&self, ctx: &BindContext, f: &mut dyn FnMut(&BindContext, &Self::Item)) {
                self.0.with(ctx, f)
            }
        }
        ReRef::from_dyn(IntoDyn(self))
    }
}


