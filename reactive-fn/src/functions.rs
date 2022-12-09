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
pub fn subscribe(mut f: impl FnMut(&mut ObsContext) + 'static) -> Subscription {
    subscribe_to((), move |_, bc| f(bc)).into_subscription()
}

#[inline]
pub fn subscribe_to<St: 'static>(
    st: St,
    mut f: impl FnMut(&mut St, &mut ObsContext) + 'static,
) -> impl Subscriber<St = St> {
    subscriber(Subscribe::new(SubscribeToData(st), move |st, bc| {
        f(&mut st.0, bc)
    }))
}

pub fn subscribe_async<Fut: Future<Output = ()> + 'static>(
    f: impl FnMut(&mut ObsContext) -> Fut + 'static,
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
    F: FnMut(&mut St, &mut ObsContext) + 'static,
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
    F: FnMut(&mut St, &mut ObsContext) + 'static,
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
    F: FnMut(&mut St, &mut ObsContext) + 'static,
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
    F: FnMut(&mut St, &mut ObsContext) + 'static,
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
    F: FnMut(&mut T, &mut ObsContext) + 'static,
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
pub fn obs<T>(f: impl Fn(&mut ObsContext) -> T + 'static) -> ImplObs<impl Observable<Item = T>> {
    struct ObsFn<F>(F);
    impl<F: Fn(&mut ObsContext) -> T + 'static, T> Observable for ObsFn<F> {
        type Item = T;

        #[inline]
        fn with<U>(
            &self,
            f: impl FnOnce(&Self::Item, &mut ObsContext) -> U,
            bc: &mut ObsContext,
        ) -> U {
            f(&(self.0)(bc), bc)
        }
    }
    ImplObs(ObsFn(f))
}

pub fn obs_with<T>(
    f: impl for<'a> Fn(ObsSink<'a, '_, '_, T>) -> Ret<'a>,
) -> ImplObs<impl Observable<Item = T>>
where
    T: ?Sized,
{
    struct ObsWithFn<T: ?Sized, F> {
        f: F,
        _phantom: PhantomData<fn(&Self) -> &T>,
    }
    impl<T, F> Observable for ObsWithFn<T, F>
    where
        T: ?Sized,
        F: for<'a> Fn(ObsSink<'a, '_, '_, T>) -> Ret<'a>,
    {
        type Item = T;

        #[inline]
        fn with<U>(
            &self,
            f: impl FnOnce(&Self::Item, &mut ObsContext) -> U,
            bc: &mut ObsContext,
        ) -> U {
            ObsCallback::with(|cb| self.with_dyn(cb.context(bc)), f)
        }
        fn with_dyn<'a>(&self, oc: ObsSink<'a, '_, '_, Self::Item>) -> Ret<'a> {
            (self.f)(oc)
        }

        #[inline]
        fn into_dyn(self) -> Obs<Self::Item>
        where
            Self: Sized + 'static,
        {
            Obs::new_dyn(Rc::new(self))
        }
    }
    ImplObs(ObsWithFn {
        f,
        _phantom: PhantomData,
    })
}

#[inline]
pub fn obs_constant<T: 'static>(value: T) -> ImplObs<ConstantObservable<T>> {
    ImplObs(ConstantObservable(value))
}

#[inline]
pub fn obs_static<T: ?Sized>(value: &'static T) -> ImplObs<StaticObservable<T>> {
    ImplObs(StaticObservable(value))
}

pub fn obs_from_async<Fut: Future + 'static>(
    future: Fut,
) -> ImplObs<impl Observable<Item = Poll<Fut::Output>>> {
    ImplObs(crate::obs_from_async::ObsFromAsync::new(future))
}
pub fn obs_from_stream<St: Stream + 'static>(
    initial_value: St::Item,
    stream: St,
) -> ImplObs<impl Observable<Item = St::Item>> {
    ImplObs(obs_from_stream::ObsFromStream::new(initial_value, stream))
}
