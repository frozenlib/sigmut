use crate::*;
use std::{cell::RefCell, mem, rc::Rc};

pub fn obs_scan<T: 'static>(
    initial_state: T,
    f: impl FnMut(&mut T, &mut BindContext) + 'static,
) -> Obs<impl Observable<Item = T>> {
    obs_scan_map(initial_state, f, |x| x)
}
pub fn obs_scan_map<St, T>(
    initial_state: St,
    f: impl FnMut(&mut St, &mut BindContext) + 'static,
    m: impl Fn(&St) -> &T + 'static,
) -> Obs<impl Observable<Item = T>>
where
    St: 'static,
    T: ?Sized + 'static,
{
    Obs(Scan::new(initial_state, f, m))
}

pub(crate) fn obs_filter_scan_map<St, T>(
    initial_state: St,
    f: impl FnMut(&mut St, &mut BindContext) -> bool + 'static,
    m: impl Fn(&St) -> &T + 'static,
) -> Obs<impl Observable<Item = T>>
where
    St: 'static,
    T: 'static + ?Sized,
{
    Obs(FilterScan::new(initial_state, f, m))
}

struct Scan<St, F, M> {
    data: RefCell<ScanData<St, F, M>>,
    sinks: BindSinks,
}
struct FilterScan<St, F, M> {
    data: RefCell<ScanData<St, F, M>>,
    sinks: BindSinks,
}

struct ScanData<St, F, M> {
    st: St,
    f: F,
    m: M,
    is_loaded: bool,
    bindings: Bindings,
}
impl<St, F, M, T> Scan<St, F, M>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) + 'static,
    M: Fn(&St) -> &T + 'static,
    T: ?Sized + 'static,
{
    fn new(initial_state: St, f: F, m: M) -> Rc<Self> {
        Rc::new(Self {
            data: RefCell::new(ScanData {
                st: initial_state,
                f,
                m,
                is_loaded: false,
                bindings: Bindings::new(),
            }),
            sinks: BindSinks::new(),
        })
    }
    fn load(self: &Rc<Self>, scope: &BindScope) {
        let mut b = &mut *self.data.borrow_mut();
        let f = &mut b.f;
        let st = &mut b.st;
        b.bindings.update(scope, &self, |cx| f(st, cx));
        b.is_loaded = true;
    }
}
impl<St, F, M, T> Observable for Rc<Scan<St, F, M>>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) + 'static,
    M: Fn(&St) -> &T + 'static,
    T: ?Sized + 'static,
{
    type Item = T;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        cx.bind(self.clone());
        let mut b = self.data.borrow();
        if !b.is_loaded {
            drop(b);
            self.load(cx.scope());
            b = self.data.borrow();
        }
        f((b.m)(&b.st), cx)
    }

    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        DynObs::from_dyn_inner(self)
    }
}

impl<St, F, M, T> BindSource for Scan<St, F, M>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) + 'static,
    M: Fn(&St) -> &T + 'static,
    T: ?Sized + 'static,
{
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
    fn detach_sink(&self, idx: usize) {
        self.sinks.detach(idx);
        if self.sinks.is_empty() {
            let d = &mut *self.data.borrow_mut();
            d.bindings.clear();
            d.is_loaded = false;
        }
    }
}
impl<St, F, M, T> BindSink for Scan<St, F, M>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) + 'static,
    M: Fn(&St) -> &T + 'static,
    T: ?Sized + 'static,
{
    fn notify(self: Rc<Self>, scope: &NotifyScope) {
        if mem::replace(&mut self.data.borrow_mut().is_loaded, false) {
            self.sinks.notify(scope)
        }
    }
}

impl<St, F, M, T> FilterScan<St, F, M>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) -> bool + 'static,
    M: Fn(&St) -> &T + 'static,
    T: ?Sized + 'static,
{
    fn new(initial_state: St, f: F, m: M) -> Rc<Self> {
        Rc::new(Self {
            data: RefCell::new(ScanData {
                st: initial_state,
                f,
                m,
                is_loaded: false,
                bindings: Bindings::new(),
            }),
            sinks: BindSinks::new(),
        })
    }
    fn load(self: &Rc<Self>, scope: &BindScope) -> bool {
        let mut b = &mut *self.data.borrow_mut();
        let f = &mut b.f;
        let st = &mut b.st;
        b.is_loaded = true;
        b.bindings.update(scope, self, |cx| f(st, cx))
    }
}
impl<St, F, M, T> Observable for Rc<FilterScan<St, F, M>>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) -> bool + 'static,
    M: Fn(&St) -> &T + 'static,
    T: ?Sized + 'static,
{
    type Item = T;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        let mut b = self.data.borrow();
        if !b.is_loaded {
            drop(b);
            self.load(cx.scope());
            b = self.data.borrow();
        }
        cx.bind(self.clone());
        f((b.m)(&b.st), cx)
    }

    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        DynObs::from_dyn_inner(self)
    }
}
impl<St, F, M, T> BindSource for FilterScan<St, F, M>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) -> bool + 'static,
    M: Fn(&St) -> &T + 'static,
    T: ?Sized + 'static,
{
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
    fn detach_sink(&self, idx: usize) {
        self.sinks.detach(idx);
        if self.sinks.is_empty() {
            let d = &mut *self.data.borrow_mut();
            d.bindings.clear();
            d.is_loaded = false;
        }
    }
}
impl<St, F, M, T> BindSink for FilterScan<St, F, M>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) -> bool + 'static,
    M: Fn(&St) -> &T + 'static,
    T: ?Sized + 'static,
{
    fn notify(self: Rc<Self>, scope: &NotifyScope) {
        if mem::replace(&mut self.data.borrow_mut().is_loaded, false) && !self.sinks.is_empty() {
            scope.defer_bind(self);
        }
    }
}
impl<St, F, M, T> BindTask for FilterScan<St, F, M>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) -> bool + 'static,
    M: Fn(&St) -> &T + 'static,
    T: ?Sized + 'static,
{
    fn run(self: Rc<Self>, scope: &BindScope) {
        if !self.data.borrow().is_loaded {
            if self.load(scope) {
                scope.defer_notify(self);
            }
        }
    }
}
