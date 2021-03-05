use crate::*;
use std::{
    any::Any,
    cell::{Ref, RefCell, RefMut},
    marker::PhantomData,
    mem,
    rc::Rc,
};

#[inline]
pub fn subscribe(f: impl FnMut(&mut BindContext) + 'static) -> Subscription {
    struct Subscribe<F>(RefCell<SubscribeInner<F>>);
    struct SubscribeInner<F> {
        f: F,
        bindings: Bindings,
        loaded: bool,
    }
    impl<F> Subscribe<F>
    where
        F: FnMut(&mut BindContext) + 'static,
    {
        fn new(f: F) -> Option<Rc<dyn Any>> {
            let s = Rc::new(Subscribe(RefCell::new(SubscribeInner {
                f,
                bindings: Bindings::new(),
                loaded: false,
            })));
            let this = s.clone();
            if BindScope::with(move |scope| this.run(scope)) {
                Some(s)
            } else {
                None
            }
        }

        fn run(self: Rc<Self>, scope: &BindScope) -> bool {
            let b = &mut *self.0.borrow_mut();
            let f = &mut b.f;
            b.bindings.update(scope, &self, |cx| f(cx));
            b.loaded = true;
            !b.bindings.is_empty()
        }
    }
    impl<F> BindSink for Subscribe<F>
    where
        F: FnMut(&mut BindContext) + 'static,
    {
        fn notify(self: Rc<Self>, scope: &NotifyScope) {
            let mut b = self.0.borrow_mut();
            if mem::replace(&mut b.loaded, false) && !b.bindings.is_empty() {
                drop(b);
                scope.defer_bind(self)
            }
        }
    }
    impl<F> BindTask for Subscribe<F>
    where
        F: FnMut(&mut BindContext) + 'static,
    {
        fn run(self: Rc<Self>, scope: &BindScope) {
            Subscribe::run(self, scope);
        }
    }
    Subscription(Subscribe::new(f))
}

pub fn subscribe_to<St: 'static>(
    st: St,
    f: impl Fn(&mut St, &mut BindContext) + 'static,
) -> impl Subscriber<St> {
    match SubscribeTo::new(st, f) {
        Ok(s) => MayConstantSubscriber::Subscriber(subscriber(s)),
        Err(st) => MayConstantSubscriber::Constant(RefCell::new(st)),
    }
}

struct SubscribeTo<St, F>(RefCell<SubscribeToData<St, F>>);
struct SubscribeToData<St, F> {
    st: St,
    f: F,
    bindings: Bindings,
    loaded: bool,
}
impl<St, F> SubscribeTo<St, F>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) + 'static,
{
    fn new(st: St, f: F) -> Result<Rc<Self>, St> {
        let s = Rc::new(Self(RefCell::new(SubscribeToData {
            st,
            f,
            bindings: Bindings::new(),
            loaded: false,
        })));
        let this = s.clone();
        if BindScope::with(move |scope| this.run(scope)) {
            Ok(s)
        } else {
            match Rc::try_unwrap(s) {
                Ok(s) => Err(s.0.into_inner().st),
                Err(s) => Ok(s),
            }
        }
    }

    fn run(self: Rc<Self>, scope: &BindScope) -> bool {
        let b = &mut *self.0.borrow_mut();
        let st = &mut b.st;
        let f = &mut b.f;
        b.bindings.update(scope, &self, |cx| f(st, cx));
        b.loaded = true;
        !b.bindings.is_empty()
    }
}
impl<St, F> BindSink for SubscribeTo<St, F>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) + 'static,
{
    fn notify(self: Rc<Self>, scope: &NotifyScope) {
        let mut b = self.0.borrow_mut();
        if mem::replace(&mut b.loaded, false) && !b.bindings.is_empty() {
            drop(b);
            scope.defer_bind(self)
        }
    }
}
impl<St, F> BindTask for SubscribeTo<St, F>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) + 'static,
{
    fn run(self: Rc<Self>, scope: &BindScope) {
        SubscribeTo::run(self, scope);
    }
}
impl<St, F> InnerSubscriber<St> for SubscribeTo<St, F>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) + 'static,
{
    fn borrow(&self) -> std::cell::Ref<St> {
        Ref::map(self.0.borrow(), |x| &x.st)
    }
    fn borrow_mut(&self) -> std::cell::RefMut<St> {
        RefMut::map(self.0.borrow_mut(), |x| &mut x.st)
    }
    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}

#[inline]
pub fn obs<T>(f: impl Fn(&mut BindContext) -> T + 'static) -> Obs<impl Observable<Item = T>> {
    struct ObsFn<F>(F);
    impl<F: Fn(&mut BindContext) -> T + 'static, T> Observable for ObsFn<F> {
        type Item = T;

        #[inline]
        fn with<U>(
            &self,
            f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
            cx: &mut BindContext,
        ) -> U {
            f(&(self.0)(cx), cx)
        }
    }
    Obs(ObsFn(f))
}

pub fn obs_with<S, T>(
    this: S,
    f: impl Fn(&S, &mut dyn FnMut(&T, &mut BindContext), &mut BindContext) + 'static,
) -> Obs<impl Observable<Item = T>>
where
    S: 'static,
    T: ?Sized + 'static,
{
    struct ObsWithFn<S, T: ?Sized, F> {
        this: S,
        f: F,
        _phantom: PhantomData<fn(&Self) -> &T>,
    }
    impl<S, T, F> Observable for ObsWithFn<S, T, F>
    where
        S: 'static,
        T: 'static + ?Sized,
        F: Fn(&S, &mut dyn FnMut(&T, &mut BindContext), &mut BindContext) + 'static,
    {
        type Item = T;

        #[inline]
        fn with<U>(
            &self,
            f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
            cx: &mut BindContext,
        ) -> U {
            let mut output = None;
            let mut f = Some(f);
            self.dyn_with(
                &mut |value, cx| output = Some((f.take().unwrap())(value, cx)),
                cx,
            );
            output.unwrap()
        }

        #[inline]
        fn into_dyn(self) -> DynObs<Self::Item>
        where
            Self: Sized,
        {
            DynObs::from_dyn(Rc::new(self))
        }
    }

    impl<S, T, F> DynamicObservable for ObsWithFn<S, T, F>
    where
        S: 'static,
        T: 'static + ?Sized,
        F: Fn(&S, &mut dyn FnMut(&T, &mut BindContext), &mut BindContext) + 'static,
    {
        type Item = T;
        fn dyn_with(&self, f: &mut dyn FnMut(&T, &mut BindContext), cx: &mut BindContext) {
            (self.f)(&self.this, f, cx)
        }
    }

    Obs(ObsWithFn {
        this,
        f,
        _phantom: PhantomData,
    })
}

#[inline]
pub fn obs_constant<T: 'static>(value: T) -> Obs<impl Observable<Item = T>> {
    struct ObsConstant<T>(T);
    impl<T: 'static> Observable for ObsConstant<T> {
        type Item = T;

        #[inline]
        fn with<U>(
            &self,
            f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
            cx: &mut BindContext,
        ) -> U {
            f(&self.0, cx)
        }
        #[inline]
        fn into_dyn(self) -> DynObs<Self::Item> {
            DynObs::new_constant(self.0)
        }
    }
    Obs(ObsConstant(value))
}

#[inline]
pub fn obs_static<T: ?Sized>(value: &'static T) -> Obs<impl Observable<Item = T>> {
    struct ObsStatic<T: ?Sized + 'static>(&'static T);
    impl<T: ?Sized> Observable for ObsStatic<T> {
        type Item = T;

        #[inline]
        fn with<U>(
            &self,
            f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
            cx: &mut BindContext,
        ) -> U {
            f(self.0, cx)
        }
        #[inline]
        fn into_dyn(self) -> DynObs<Self::Item> {
            DynObs::new_static(self.0)
        }
    }

    Obs(ObsStatic(value))
}
