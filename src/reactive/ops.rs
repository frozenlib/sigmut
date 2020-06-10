use super::*;
use std::cell::Ref;

pub trait Reactive: 'static {
    type Item;
    fn get(&self, ctx: &BindContext) -> Self::Item;

    fn into_dyn_re(self) -> Re<Self::Item>
    where
        Self: Sized,
    {
        todo!();
    }
    fn into_dyn_re_ref(self) -> ReRef<Self::Item>
    where
        Self: Sized,
    {
        todo!();
    }
}

pub trait ReactiveBorrow: 'static {
    type Item: ?Sized;
    fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item>;

    fn into_dyn_re_borrow(self) -> ReBorrow<Self::Item>;
    fn into_dyn_re_ref(self) -> ReRef<Self::Item>;
}

pub trait ReactiveRef: 'static {
    type Item: ?Sized;
    fn with(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &Self::Item));

    fn into_dyn_re_ref(self) -> ReRef<Self::Item>;
}

pub struct ReOps<S>(S);
pub struct ReBorrowOps<S>(S);
pub struct ReRefOps<S>(S);

// pub fn new_re<T>(f: impl Fn(&BindContext) -> T) -> ReOps<impl Reactive<Item = T>> {
//     todo!();
// }
// pub fn new_re_borrow<T>(
//     f: impl for<'a> Fn(&BindContext<'a>) -> Ref<'a, T>,
// ) -> ReOps<impl ReactiveBorrow<Item = T>> {
//     todo!();
// }
// pub fn new_re_ref<T, U>(
//     f: impl for<'a> Fn(&BindContext<'a>, impl Fn(&BindContext<'a>, &T) -> U) -> U,
// ) -> ReOps<impl ReactiveRef<Item = T>> {
//     todo!();
// }

impl<S: Reactive> ReOps<S> {}
impl<S: ReactiveBorrow> ReBorrowOps<S> {}
impl<S: ReactiveRef> ReRefOps<S> {}

struct ReFn<F>(F);
impl<F: Fn(&BindContext) -> T + 'static, T> Reactive for ReFn<F> {
    type Item = T;
    fn get(&self, ctx: &BindContext) -> Self::Item {
        (self.0)(ctx)
    }
}
