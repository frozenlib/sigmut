use crate::observables::*;
use crate::*;
use futures_core::Stream;
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
    subscribe_to((), move |_, bc| f(bc)).into_subscription()
}

#[inline]
pub fn subscribe_to<St: 'static>(
    st: St,
    mut f: impl FnMut(&mut St, &mut BindContext) + 'static,
) -> impl Subscriber<St = St> {
    subscriber(Subscribe::new(SubscribeToData(st), move |st, bc| {
        f(&mut st.0, bc)
    }))
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
    pub(crate) fn new(st: St, f: F) -> Rc<Self> {
        Self::new_with(st, f, false, Bindings::new())
    }
    pub(crate) fn new_tail(st: St, f: F, is_modified: bool, bindings: Bindings) -> Rc<Self> {
        Self::new_with(st, f, !is_modified, bindings)
    }
    fn new_with(st: St, f: F, is_loaded: bool, bindings: Bindings) -> Rc<Self> {
        let this = Rc::new(Self(RefCell::new(SubscribeData {
            st,
            f,
            is_loaded,
            bindings,
        })));
        if !is_loaded {
            schedule_bind(&this);
        }
        this
    }

    fn ready(self: &Rc<Self>, scope: &BindScope) {
        if !self.0.borrow().is_loaded {
            self.load(scope);
        }
    }
    fn load(self: &Rc<Self>, scope: &BindScope) -> bool {
        let b = &mut *self.0.borrow_mut();
        b.bindings.update(scope, self, |bc| (b.f)(&mut b.st, bc));
        b.is_loaded = true;
        !b.bindings.is_empty()
    }
}
impl<St, F> BindSink for Subscribe<St, F>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) + 'static,
{
    fn notify(self: Rc<Self>, _scope: &NotifyScope) {
        let mut b = self.0.borrow_mut();
        if mem::replace(&mut b.is_loaded, false) && !b.bindings.is_empty() {
            schedule_bind(&self);
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
    fn to_subscription(self: Rc<Self>) -> Subscription {
        Subscription(Some(self))
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
            bc: &mut BindContext,
        ) -> U {
            f(&(self.0)(bc), bc)
        }
    }
    Obs(ObsFn(f))
}

pub fn obs_with<S, T>(
    this: S,
    f: impl for<'a> Fn(&S, ObsContext<'a, '_, '_, T>) -> Ret<'a> + 'static,
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
        F: for<'a> Fn(&S, ObsContext<'a, '_, '_, T>) -> Ret<'a> + 'static,
    {
        type Item = T;

        #[inline]
        fn with<U>(
            &self,
            f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
            bc: &mut BindContext,
        ) -> U {
            ObsCallback::with(|cb| self.with_dyn(cb.context(bc)), f)
        }
        fn with_dyn<'a>(&self, oc: ObsContext<'a, '_, '_, Self::Item>) -> Ret<'a> {
            (self.f)(&self.this, oc)
        }

        #[inline]
        fn into_dyn(self) -> DynObs<Self::Item>
        where
            Self: Sized,
        {
            DynObs::new_dyn(Rc::new(self))
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
    Obs(ConstantObservable(value))
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
