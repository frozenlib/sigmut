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

    fn into_dyn_re_borrow(self) -> ReBorrow<Self::Item>
    where
        Self: Sized,
    {
        todo!()
    }
    fn into_dyn_re_ref(self) -> ReRef<Self::Item>
    where
        Self: Sized,
    {
        todo!()
    }
}

pub trait ReactiveRef: 'static {
    type Item: ?Sized;
    fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &Self::Item) -> U) -> U;

    fn into_dyn_re_ref(self) -> ReRef<Self::Item>
    where
        Self: Sized,
    {
        todo!()
    }
}

pub fn re<T>(f: impl Fn(&BindContext) -> T + 'static) -> ReOps<impl Reactive<Item = T>> {
    struct ReFn<F>(F);
    impl<F: Fn(&BindContext) -> T + 'static, T> Reactive for ReFn<F> {
        type Item = T;
        fn get(&self, ctx: &BindContext) -> Self::Item {
            (self.0)(ctx)
        }
    }

    ReOps(ReFn(f))
}
pub fn re_constant<T: 'static + Clone>(value: T) -> ReOps<impl Reactive<Item = T>> {
    re(move |_| value.clone())
}

pub struct ReOps<S>(S);
pub struct ReBorrowOps<S>(S);
pub struct ReRefOps<S>(S);

pub fn re_borrow<S, T>(
    this: S,
    borrow: impl for<'a> Fn(&'a S, &BindContext<'a>) -> Ref<'a, T> + 'static,
) -> ReBorrowOps<impl ReactiveBorrow<Item = T>>
where
    T: 'static + ?Sized,
    S: 'static,
{
    struct ReBorrowFn<S, F> {
        this: S,
        borrow: F,
    }
    impl<T, S, F> ReactiveBorrow for ReBorrowFn<S, F>
    where
        T: 'static + ?Sized,
        S: 'static,
        for<'a> F: Fn(&'a S, &BindContext<'a>) -> Ref<'a, T> + 'static,
    {
        type Item = T;
        fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, T> {
            (self.borrow)(&self.this, ctx)
        }
    }

    ReBorrowOps(ReBorrowFn { this, borrow })
}
pub fn re_borrow_constant<T: 'static>(value: T) -> ReBorrowOps<impl ReactiveBorrow<Item = T>> {
    re_borrow(RefCell::new(value), |this, _| this.borrow())
}

pub fn re_ref_constant<T: 'static>(value: T) -> ReRefOps<impl ReactiveRef<Item = T>> {
    struct ReRefConstant<T>(T);
    impl<T: 'static> ReactiveRef for ReRefConstant<T> {
        type Item = T;
        fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &Self::Item) -> U) -> U {
            f(ctx, &self.0)
        }
    }
    ReRefOps(ReRefConstant(value))
}

impl<S: Reactive> ReOps<S> {}
impl<S: ReactiveBorrow> ReBorrowOps<S> {}
impl<S: ReactiveRef> ReRefOps<S> {}
