use crate::observables::*;
use crate::*;
use futures::Stream;
use std::{
    any::Any,
    cell::{Ref, RefCell, RefMut},
    future::Future,
    marker::PhantomData,
    mem,
    rc::Rc,
    task::Poll,
};

#[inline]
pub fn subscribe(mut f: impl FnMut(&mut BindContext) + 'static) -> Subscription {
    subscribe_to((), move |_, cx| f(cx)).into_subscription()
}

#[inline]
pub fn subscribe_to<St: 'static>(
    st: St,
    mut f: impl FnMut(&mut St, &mut BindContext) + 'static,
) -> impl Subscriber<St = St> {
    match Subscribe::new(SubscribeToData(st), move |st, cx| f(&mut st.0, cx)) {
        Ok(s) => MayConstantSubscriber::Subscriber(subscriber(s)),
        Err(st) => MayConstantSubscriber::Constant(RefCell::new(st.0)),
    }
}

pub fn subscribe_async<Fut: Future<Output = ()> + 'static>(
    f: impl FnMut(&mut BindContext) -> Fut + 'static,
) -> Subscription {
    Subscription(Some(subscribe_async::SubscribeAsync::new(f)))
}

struct SubscribeToData<St>(St);
impl<St> SubscriberState for SubscribeToData<St> {
    type St = St;

    fn borrow(&self) -> &Self::St {
        &self.0
    }
    fn borrow_mut(&mut self) -> &mut Self::St {
        &mut self.0
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
            is_loaded: false,
            bindings: Bindings::new(),
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
    pub(crate) fn new_tail(st: St, f: F, is_modified: bool, bindings: Bindings) -> Rc<Self> {
        let s = Rc::new(Self(RefCell::new(SubscribeData {
            st,
            f,
            is_loaded: !is_modified,
            bindings,
        })));
        if is_modified {
            BindScope::with(|scope| s.load(scope));
        }
        s
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

pub(crate) trait SubscriberState {
    type St;
    fn borrow(&self) -> &Self::St;
    fn borrow_mut(&mut self) -> &mut Self::St;
}
impl<St, F> InnerSubscriber for Subscribe<St, F>
where
    St: SubscriberState + 'static,
    F: FnMut(&mut St, &mut BindContext) + 'static,
{
    type St = St::St;
    fn borrow(&self) -> std::cell::Ref<Self::St> {
        Ref::map(self.0.borrow(), |x| x.st.borrow())
    }
    fn borrow_mut(&self) -> std::cell::RefMut<Self::St> {
        RefMut::map(self.0.borrow_mut(), |x| x.st.borrow_mut())
    }
    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}

pub(crate) trait FoldState {
    type Output;
    fn finish(&mut self) -> Self::Output;
}
impl<T> FoldState for Option<T> {
    type Output = T;
    fn finish(&mut self) -> Self::Output {
        self.take().unwrap()
    }
}

impl<T: FoldState, F> DynamicFold for Subscribe<T, F>
where
    T: 'static,
    F: FnMut(&mut T, &mut BindContext) + 'static,
{
    type Output = T::Output;

    fn stop(self: Rc<Self>, scope: &BindScope) -> Self::Output {
        self.ready(scope);
        let mut s = self.0.borrow_mut();
        s.bindings.clear();
        s.st.finish()
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
            DynObs::new_dyn(Rc::new(self))
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
pub fn obs_constant<T: 'static>(value: T) -> Obs<ConstantObservable<T>> {
    Obs(ConstantObservable::new(value))
}

#[inline]
pub fn obs_static<T: ?Sized>(value: &'static T) -> Obs<StaticObservable<T>> {
    Obs(StaticObservable(value))
}

pub fn obs_from_async<Fut: Future + 'static>(
    future: Fut,
) -> Obs<impl Observable<Item = Poll<Fut::Output>>> {
    Obs(crate::obs_from_async::ObsFromAsync::new(future))
}
pub fn obs_from_stream<St: Stream + 'static>(
    initial_value: St::Item,
    stream: St,
) -> Obs<impl Observable<Item = St::Item>> {
    Obs(obs_from_stream::ObsFromStream::new(initial_value, stream))
}
