use crate::*;
use std::{cell::RefCell, mem, rc::Rc};

pub fn obs_scan<T: 'static>(
    initial_state: T,
    f: impl FnMut(&mut T, &mut BindContext) + 'static,
) -> Obs<impl Observable<Item = T>> {
    obs_scan_with(initial_state, f, MapId)
}
pub fn obs_scan_map<St, T>(
    initial_state: St,
    f: impl FnMut(&mut St, &mut BindContext) + 'static,
    m: impl Fn(&St) -> T + 'static,
) -> Obs<impl Observable<Item = T>>
where
    St: 'static,
    T: 'static,
{
    obs_scan_with(initial_state, f, MapValue(m))
}
pub fn obs_scan_map_ref<St, T>(
    initial_state: St,
    f: impl FnMut(&mut St, &mut BindContext) + 'static,
    m: impl Fn(&St) -> &T + 'static,
) -> Obs<impl Observable<Item = T>>
where
    St: 'static,
    T: ?Sized + 'static,
{
    obs_scan_with(initial_state, f, MapRef(m))
}
pub(crate) fn obs_scan_with<St: 'static, M: Map<St>>(
    initial_state: St,
    f: impl FnMut(&mut St, &mut BindContext) + 'static,
    m: M,
) -> Obs<impl Observable<Item = M::Output>> {
    Obs(Scan::new(initial_state, f, m))
}

pub(crate) fn obs_filter_scan_with<St: 'static, M: Map<St>>(
    initial_state: St,
    f: impl FnMut(&mut St, &mut BindContext) -> bool + 'static,
    m: M,
) -> Obs<impl Observable<Item = M::Output>> {
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
impl<St, F, M> Scan<St, F, M>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) + 'static,
    M: Map<St>,
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
        b.bindings.update(scope, self, |bc| f(st, bc));
        b.is_loaded = true;
    }
}
impl<St, F, M> Observable for Rc<Scan<St, F, M>>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) + 'static,
    M: Map<St>,
{
    type Item = M::Output;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        bc: &mut BindContext,
    ) -> U {
        bc.bind(self.clone());
        let mut b = self.data.borrow();
        if !b.is_loaded {
            drop(b);
            self.load(bc.scope());
            b = self.data.borrow();
        }
        b.m.map(&b.st, |value| f(value, bc))
    }

    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        DynObs::new_dyn_inner(self)
    }
}

impl<St, F, M> BindSource for Scan<St, F, M>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) + 'static,
    M: Map<St>,
{
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
    fn on_sinks_empty(self: Rc<Self>) {
        let d = &mut *self.data.borrow_mut();
        d.bindings.clear();
        d.is_loaded = false;
    }
}
impl<St, F, M> BindSink for Scan<St, F, M>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) + 'static,
    M: Map<St>,
{
    fn notify(self: Rc<Self>, scope: &NotifyScope) {
        if mem::replace(&mut self.data.borrow_mut().is_loaded, false) {
            self.sinks.notify(scope)
        }
    }
}

impl<St, F, M> FilterScan<St, F, M>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) -> bool + 'static,
    M: Map<St>,
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
        b.bindings.update(scope, self, |bc| f(st, bc))
    }
}
impl<St, F, M> Observable for Rc<FilterScan<St, F, M>>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) -> bool + 'static,
    M: Map<St>,
{
    type Item = M::Output;

    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        bc: &mut BindContext,
    ) -> U {
        let mut b = self.data.borrow();
        if !b.is_loaded {
            drop(b);
            self.load(bc.scope());
            b = self.data.borrow();
        }
        bc.bind(self.clone());
        b.m.map(&b.st, |value| f(value, bc))
    }

    fn into_dyn(self) -> DynObs<Self::Item>
    where
        Self: Sized,
    {
        DynObs::new_dyn_inner(self)
    }
}
impl<St, F, M> BindSource for FilterScan<St, F, M>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) -> bool + 'static,
    M: Map<St>,
{
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
    fn on_sinks_empty(self: Rc<Self>) {
        let d = &mut *self.data.borrow_mut();
        d.bindings.clear();
        d.is_loaded = false;
    }
}
impl<St, F, M> BindSink for FilterScan<St, F, M>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) -> bool + 'static,
    M: Map<St>,
{
    fn notify(self: Rc<Self>, scope: &NotifyScope) {
        if mem::replace(&mut self.data.borrow_mut().is_loaded, false) && !self.sinks.is_empty() {
            scope.defer_bind(self);
        }
    }
}
impl<St, F, M> BindTask for FilterScan<St, F, M>
where
    St: 'static,
    F: FnMut(&mut St, &mut BindContext) -> bool + 'static,
    M: Map<St>,
{
    fn run(self: Rc<Self>, scope: &BindScope) {
        if !self.data.borrow().is_loaded {
            // Cannot be combined into a single `if`
            // because return value of `borrow()` need to be released before `self.load()`.
            if self.load(scope) {
                scope.defer_notify(self);
            }
        }
    }
}
