use super::*;
use std::cell::Ref;

pub trait Reactive: 'static {
    type Item;
    fn get(&self, ctx: &BindContext) -> Self::Item;

    fn into_dyn(self) -> Re<Self::Item>
    where
        Self: Sized,
    {
        todo!();
    }
}

pub trait ReactiveBorrow: 'static {
    type Item: ?Sized;
    fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item>;

    fn into_dyn(self) -> ReBorrow<Self::Item>
    where
        Self: Sized,
    {
        todo!()
    }
}

pub trait ReactiveRef: 'static {
    type Item: ?Sized;
    fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &Self::Item) -> U) -> U;

    fn into_dyn(self) -> ReRef<Self::Item>
    where
        Self: Sized,
    {
        todo!()
    }
}

pub fn re<T>(get: impl Fn(&BindContext) -> T + 'static) -> ReOps<impl Reactive<Item = T>> {
    struct ReFn<F>(F);
    impl<F: Fn(&BindContext) -> T + 'static, T> Reactive for ReFn<F> {
        type Item = T;
        fn get(&self, ctx: &BindContext) -> Self::Item {
            (self.0)(ctx)
        }
    }

    ReOps(ReFn(get))
}
pub fn re_constant<T: 'static + Clone>(value: T) -> ReOps<impl Reactive<Item = T>> {
    re(move |_| value.clone())
}

#[derive(Clone)]
pub struct ReOps<S>(S);

impl<S: Reactive> ReOps<S> {
    pub fn get(&self, ctx: &BindContext) -> S::Item {
        self.0.get(ctx)
    }
    pub fn with<T>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &S::Item) -> T) -> T {
        f(ctx, &self.get(ctx))
    }
    pub fn map<T>(self, f: impl Fn(S::Item) -> T + 'static) -> ReOps<impl Reactive<Item = T>> {
        re(move |ctx| f(self.get(ctx)))
    }
    pub fn flat_map<T: Reactive>(
        self,
        f: impl Fn(S::Item) -> T + 'static,
    ) -> ReOps<impl Reactive<Item = T::Item>> {
        re(move |ctx| f(self.get(ctx)).get(ctx))
    }

    pub fn into_ref(self) -> ReRefOps<impl ReactiveRef<Item = S::Item>> {
        struct ReRefByRe<S>(ReOps<S>);
        impl<S: Reactive> ReactiveRef for ReRefByRe<S> {
            type Item = S::Item;
            fn with<U>(
                &self,
                ctx: &BindContext,
                f: impl FnOnce(&BindContext, &Self::Item) -> U,
            ) -> U {
                self.0.with(ctx, f)
            }
            fn into_dyn(self) -> ReRef<Self::Item>
            where
                Self: Sized,
            {
                self.0.into_dyn_ref()
            }
        }
        ReRefOps(ReRefByRe(self))
    }
    pub fn into_dyn(self) -> Re<S::Item> {
        self.0.into_dyn()
    }
    pub fn into_dyn_ref(self) -> ReRef<S::Item> {
        self.0.into_dyn().to_re_ref()
    }
}
impl<S: Reactive> Reactive for ReOps<S> {
    type Item = S::Item;
    fn get(&self, ctx: &BindContext) -> Self::Item {
        self.0.get(ctx)
    }
    fn into_dyn(self) -> Re<Self::Item>
    where
        Self: Sized,
    {
        self.0.into_dyn()
    }
}

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

#[derive(Clone)]
pub struct ReBorrowOps<S>(S);

impl<S: ReactiveBorrow> ReBorrowOps<S> {
    pub fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, S::Item> {
        self.0.borrow(ctx)
    }
    pub fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &S::Item) -> U) -> U {
        f(ctx, &self.borrow(ctx))
    }
    pub fn into_ref(self) -> ReRefOps<impl ReactiveRef<Item = S::Item>> {
        struct ReRefByReBorrow<S>(ReBorrowOps<S>);
        impl<S: ReactiveBorrow> ReactiveRef for ReRefByReBorrow<S> {
            type Item = S::Item;
            fn with<U>(
                &self,
                ctx: &BindContext,
                f: impl FnOnce(&BindContext, &Self::Item) -> U,
            ) -> U {
                self.0.with(ctx, f)
            }
            fn into_dyn(self) -> ReRef<Self::Item>
            where
                Self: Sized,
            {
                self.0.into_dyn_ref()
            }
        }
        ReRefOps(ReRefByReBorrow(self))
    }
    pub fn into_dyn(self) -> ReBorrow<S::Item> {
        self.0.into_dyn()
    }
    pub fn into_dyn_ref(self) -> ReRef<S::Item> {
        self.into_dyn().to_re_ref()
    }
}
impl<S: ReactiveBorrow> ReactiveBorrow for ReBorrowOps<S> {
    type Item = S::Item;
    fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.0.borrow(ctx)
    }
    fn into_dyn(self) -> ReBorrow<Self::Item>
    where
        Self: Sized,
    {
        self.0.into_dyn()
    }
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

#[derive(Clone)]
pub struct ReRefOps<S>(S);

impl<S: ReactiveRef> ReRefOps<S> {
    pub fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &S::Item) -> U) -> U {
        self.0.with(ctx, f)
    }
    pub fn into_dyn(self) -> ReRef<S::Item> {
        self.0.into_dyn()
    }
}
impl<S: ReactiveRef> ReactiveRef for ReRefOps<S> {
    type Item = S::Item;
    fn with<U>(&self, ctx: &BindContext, f: impl FnOnce(&BindContext, &Self::Item) -> U) -> U {
        self.0.with(ctx, f)
    }
    fn into_dyn(self) -> ReRef<Self::Item>
    where
        Self: Sized,
    {
        self.0.into_dyn()
    }
}
