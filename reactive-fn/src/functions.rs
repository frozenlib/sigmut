use crate::*;
use std::{
    any::Any,
    cell::{Ref, RefCell, RefMut},
    marker::PhantomData,
    mem,
    rc::Rc,
};

#[inline]
pub fn subscribe(mut f: impl FnMut(&mut BindContext) + 'static) -> Subscription {
    subscribe_to((), move |_, cx| f(cx)).into_subscription()
}

#[inline]
pub fn subscribe_to<St: 'static>(
    st: St,
    f: impl FnMut(&mut St, &mut BindContext) + 'static,
) -> impl Subscriber<St> {
    match Subscribe::new(st, f) {
        Ok(s) => MayConstantSubscriber::Subscriber(subscriber(s)),
        Err(st) => MayConstantSubscriber::Constant(RefCell::new(st)),
    }
}

pub(crate) struct Subscribe<St, F>(RefCell<SubscribeData<St, F>>);
struct SubscribeData<St, F> {
    st: St,
    f: F,
    bindings: Bindings,
    is_loaded: bool,
}
impl<St, F> Subscribe<St, F>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) + 'static,
{
    pub(crate) fn new(st: St, f: F) -> Result<Rc<Self>, St> {
        let s = Rc::new(Self(RefCell::new(SubscribeData {
            st,
            f,
            bindings: Bindings::new(),
            is_loaded: false,
        })));
        if BindScope::with(|scope| s.load(scope)) {
            Ok(s)
        } else {
            match Rc::try_unwrap(s) {
                Ok(s) => Err(s.0.into_inner().st),
                Err(s) => Ok(s),
            }
        }
    }

    fn ready(self: &Rc<Self>, scope: &BindScope) {
        if !self.0.borrow().is_loaded {
            self.load(scope);
        }
    }
    fn load(self: &Rc<Self>, scope: &BindScope) -> bool {
        let b = &mut *self.0.borrow_mut();
        let st = &mut b.st;
        let f = &mut b.f;
        b.bindings.update(scope, self, |cx| f(st, cx));
        b.is_loaded = true;
        !b.bindings.is_empty()
    }
}
impl<St, F> BindSink for Subscribe<St, F>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) + 'static,
{
    fn notify(self: Rc<Self>, scope: &NotifyScope) {
        let mut b = self.0.borrow_mut();
        if mem::replace(&mut b.is_loaded, false) && !b.bindings.is_empty() {
            drop(b);
            scope.defer_bind(self)
        }
    }
}
impl<St, F> BindTask for Subscribe<St, F>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) + 'static,
{
    fn run(self: Rc<Self>, scope: &BindScope) {
        Subscribe::load(&self, scope);
    }
}
impl<St, F> InnerSubscriber<St> for Subscribe<St, F>
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

impl<T, F> DynamicFold for Subscribe<Option<T>, F>
where
    T: 'static,
    F: FnMut(&mut Option<T>, &mut BindContext) + 'static,
{
    type Output = T;

    fn stop(self: Rc<Self>, scope: &BindScope) -> Self::Output {
        self.ready(scope);
        let mut s = self.0.borrow_mut();
        s.bindings.clear();
        s.st.take().unwrap()
    }
    fn as_dyn_any(self: Rc<Self>) -> Rc<dyn Any> {
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
