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
    fn ready(self: &Rc<Self>, scope: &BindScope) {
        let mut b = &mut *self.data.borrow_mut();
        if !b.is_loaded {
            let f = &mut b.f;
            let st = &mut b.st;
            if b.bindings.update(scope, self, |cx| f(st, cx)) {
                scope.defer_notify(self.clone());
            }
            b.is_loaded = true;
        }
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
        cx.bind(self.clone());
        let mut b = self.data.borrow();
        if !b.is_loaded {
            drop(b);
            self.ready(cx.scope());
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
        self.ready(scope);
    }
}

// use super::*;
// use std::{
//     any::Any,
//     cell::{Ref, RefCell, RefMut},
//     marker::PhantomData,
//     mem::take,
//     rc::Rc,
// };

// use super::DynamicFold;

// pub trait ScanOp: 'static {
//     type LoadSt;
//     type UnloadSt;
//     type Value;
//     fn load(&mut self, state: Self::UnloadSt, cx: &mut BindContext) -> Self::LoadSt;
//     fn unload(&mut self, state: Self::LoadSt) -> Self::UnloadSt;
//     fn get<'a>(&self, state: &'a Self::LoadSt) -> &'a Self::Value;
// }
// pub trait FilterScanOp: 'static {
//     type LoadSt;
//     type UnloadSt;
//     type Value;
//     fn load(&mut self, state: Self::UnloadSt, cx: &mut BindContext)
//         -> FilterScanLoad<Self::LoadSt>;
//     fn unload(&mut self, state: Self::LoadSt) -> Self::UnloadSt;
//     fn get<'a>(&self, state: &'a Self::LoadSt) -> &'a Self::Value;
// }
// pub trait FoldByOp: 'static {
//     type LoadSt;
//     type UnloadSt;
//     type Value;
//     fn load(&mut self, state: Self::UnloadSt, cx: &mut BindContext) -> Self::LoadSt;
//     fn unload(&mut self, state: Self::LoadSt) -> Self::UnloadSt;
//     fn get(&self, state: Self::LoadSt) -> Self::Value;
// }

// pub struct FilterScanLoad<LoadSt> {
//     pub state: LoadSt,
//     pub is_notify: bool,
// }
// struct ScanData<Op: ScanOp> {
//     op: Op,
//     state: ScanState<Op::LoadSt, Op::UnloadSt>,
//     bindings: Bindings,
// }
// struct FilterScanData<Op: FilterScanOp> {
//     op: Op,
//     state: ScanState<Op::LoadSt, Op::UnloadSt>,
//     bindings: Bindings,
// }
// struct FoldByData<Op: FoldByOp> {
//     op: Op,
//     state: ScanState<Op::LoadSt, Op::UnloadSt>,
//     bindings: Bindings,
// }

// pub enum ScanState<LoadSt, UnloadSt> {
//     NoData,
//     Loaded(LoadSt),
//     Unloaded(UnloadSt),
// }
// impl<LoadSt, UnloadSt> Default for ScanState<LoadSt, UnloadSt> {
//     fn default() -> Self {
//         Self::NoData
//     }
// }

// impl<LoadSt, UnloadSt> ScanState<LoadSt, UnloadSt> {
//     fn load(
//         &mut self,
//         bindings: &mut Bindings,
//         scope: &BindScope,
//         sink: &Rc<impl BindSink>,
//         load: impl FnOnce(UnloadSt, &mut BindContext) -> LoadSt,
//     ) -> bool {
//         if let Self::Unloaded(_) = self {
//             if let Self::Unloaded(state) = take(self) {
//                 *self = Self::Loaded(bindings.update(scope, sink, |cx| load(state, cx)));
//                 return true;
//             } else {
//                 unreachable!()
//             }
//         }
//         false
//     }

//     fn unload(&mut self, unload: impl FnOnce(LoadSt) -> UnloadSt) -> bool {
//         if let Self::Loaded(_) = self {
//             if let Self::Loaded(value) = take(self) {
//                 *self = Self::Unloaded(unload(value));
//                 return true;
//             } else {
//                 unreachable!()
//             }
//         }
//         false
//     }
//     fn is_loaded(&self) -> bool {
//         match self {
//             Self::Loaded(_) => true,
//             Self::Unloaded(_) => false,
//             Self::NoData => panic!("ScanState invalid state."),
//         }
//     }
//     fn get<'a, T>(&'a self, get: impl Fn(&'a LoadSt) -> &'a T) -> &'a T {
//         if let ScanState::Loaded(state) = &self {
//             get(state)
//         } else {
//             panic!("value not loaded.")
//         }
//     }
// }

// impl<Op: ScanOp> ScanData<Op> {
//     fn load(&mut self, scope: &BindScope, sink: &Rc<impl BindSink>) -> bool {
//         let op = &mut self.op;
//         self.state
//             .load(&mut self.bindings, scope, sink, |state, cx| {
//                 op.load(state, cx)
//             })
//     }
//     fn unload(&mut self) -> bool {
//         let op = &mut self.op;
//         self.state.unload(|state| op.unload(state))
//     }
//     fn get(&self) -> &Op::Value {
//         self.state.get(|state| self.op.get(state))
//     }
// }
// impl<Op: FilterScanOp> FilterScanData<Op> {
//     fn load(&mut self, scope: &BindScope, sink: &Rc<impl BindSink>) -> bool {
//         let mut is_notify = false;
//         let op = &mut self.op;
//         self.state
//             .load(&mut self.bindings, scope, sink, |state, cx| {
//                 let r = op.load(state, cx);
//                 is_notify = r.is_notify;
//                 r.state
//             });
//         is_notify
//     }
//     fn unload(&mut self) -> bool {
//         let op = &mut self.op;
//         self.state.unload(|state| op.unload(state))
//     }
//     fn get(&self) -> &Op::Value {
//         self.state.get(|state| self.op.get(state))
//     }
// }
// impl<Op: FoldByOp> FoldByData<Op> {
//     fn load(&mut self, scope: &BindScope, sink: &Rc<impl BindSink>) -> bool {
//         let op = &mut self.op;
//         self.state
//             .load(&mut self.bindings, scope, sink, |state, cx| {
//                 op.load(state, cx)
//             })
//     }
//     fn unload(&mut self) -> bool {
//         let op = &mut self.op;
//         self.state.unload(|state| op.unload(state))
//     }
// }

// struct AnonymousScanOp<LoadSt, UnloadSt, Value, Load, Unload, Get>
// where
//     Load: FnMut(UnloadSt, &mut BindContext) -> LoadSt,
//     Unload: FnMut(LoadSt) -> UnloadSt,
//     Get: Fn(&LoadSt) -> &Value,
// {
//     load: Load,
//     unload: Unload,
//     get: Get,
//     get_phatnom: PhantomData<fn(&LoadSt) -> &Value>,
// }
// impl<LoadSt, UnloadSt, Value, Load, Unload, Get> ScanOp
//     for AnonymousScanOp<LoadSt, UnloadSt, Value, Load, Unload, Get>
// where
//     Load: FnMut(UnloadSt, &mut BindContext) -> LoadSt + 'static,
//     Unload: FnMut(LoadSt) -> UnloadSt + 'static,
//     Get: Fn(&LoadSt) -> &Value + 'static,
//     LoadSt: 'static,
//     UnloadSt: 'static,
//     Value: 'static,
// {
//     type LoadSt = LoadSt;
//     type UnloadSt = UnloadSt;
//     type Value = Value;

//     fn load(&mut self, state: Self::UnloadSt, cx: &mut BindContext) -> Self::LoadSt {
//         (self.load)(state, cx)
//     }
//     fn unload(&mut self, state: Self::LoadSt) -> Self::UnloadSt {
//         (self.unload)(state)
//     }
//     fn get<'a>(&self, state: &'a Self::LoadSt) -> &'a Self::Value {
//         (self.get)(state)
//     }
// }
// pub fn scan_op<LoadSt: 'static, UnloadSt: 'static, Value: 'static>(
//     load: impl FnMut(UnloadSt, &mut BindContext) -> LoadSt + 'static,
//     unload: impl FnMut(LoadSt) -> UnloadSt + 'static,
//     get: impl Fn(&LoadSt) -> &Value + 'static,
// ) -> impl ScanOp<LoadSt = LoadSt, UnloadSt = UnloadSt, Value = Value> {
//     AnonymousScanOp {
//         load,
//         unload,
//         get,
//         get_phatnom: PhantomData,
//     }
// }

// struct AnonymousFilterScanOp<LoadSt, UnloadSt, Value, Load, Unload, Get>
// where
//     Load: FnMut(UnloadSt, &mut BindContext) -> FilterScanLoad<LoadSt>,
//     Unload: FnMut(LoadSt) -> UnloadSt,
//     Get: Fn(&LoadSt) -> &Value,
// {
//     load: Load,
//     unload: Unload,
//     get: Get,
//     get_phatnom: PhantomData<fn(&LoadSt) -> &Value>,
// }
// impl<LoadSt, UnloadSt, Value, Load, Unload, Get> FilterScanOp
//     for AnonymousFilterScanOp<LoadSt, UnloadSt, Value, Load, Unload, Get>
// where
//     Load: FnMut(UnloadSt, &mut BindContext) -> FilterScanLoad<LoadSt> + 'static,
//     Unload: FnMut(LoadSt) -> UnloadSt + 'static,
//     Get: Fn(&LoadSt) -> &Value + 'static,
//     LoadSt: 'static,
//     UnloadSt: 'static,
//     Value: 'static,
// {
//     type LoadSt = LoadSt;
//     type UnloadSt = UnloadSt;
//     type Value = Value;

//     fn load(
//         &mut self,
//         state: Self::UnloadSt,
//         cx: &mut BindContext,
//     ) -> FilterScanLoad<Self::LoadSt> {
//         (self.load)(state, cx)
//     }
//     fn unload(&mut self, state: Self::LoadSt) -> Self::UnloadSt {
//         (self.unload)(state)
//     }
//     fn get<'a>(&self, state: &'a Self::LoadSt) -> &'a Self::Value {
//         (self.get)(state)
//     }
// }
// pub fn filter_scan_op<LoadSt: 'static, UnloadSt: 'static, Value: 'static>(
//     load: impl FnMut(UnloadSt, &mut BindContext) -> FilterScanLoad<LoadSt> + 'static,
//     unload: impl FnMut(LoadSt) -> UnloadSt + 'static,
//     get: impl Fn(&LoadSt) -> &Value + 'static,
// ) -> impl FilterScanOp<LoadSt = LoadSt, UnloadSt = UnloadSt, Value = Value> {
//     AnonymousFilterScanOp {
//         load,
//         unload,
//         get,
//         get_phatnom: PhantomData,
//     }
// }

// struct AnonymousFoldByOp<LoadSt, UnloadSt, Value, Load, Unload, Get>
// where
//     Load: FnMut(UnloadSt, &mut BindContext) -> LoadSt,
//     Unload: FnMut(LoadSt) -> UnloadSt,
//     Get: Fn(LoadSt) -> Value,
// {
//     load: Load,
//     unload: Unload,
//     get: Get,
//     get_phatnom: PhantomData<fn(&LoadSt) -> &Value>,
// }
// impl<LoadSt, UnloadSt, Value, Load, Unload, Get> FoldByOp
//     for AnonymousFoldByOp<LoadSt, UnloadSt, Value, Load, Unload, Get>
// where
//     Load: FnMut(UnloadSt, &mut BindContext) -> LoadSt + 'static,
//     Unload: FnMut(LoadSt) -> UnloadSt + 'static,
//     Get: Fn(LoadSt) -> Value + 'static,
//     LoadSt: 'static,
//     UnloadSt: 'static,
//     Value: 'static,
// {
//     type LoadSt = LoadSt;
//     type UnloadSt = UnloadSt;
//     type Value = Value;

//     fn load(&mut self, state: Self::UnloadSt, cx: &mut BindContext) -> Self::LoadSt {
//         (self.load)(state, cx)
//     }
//     fn unload(&mut self, state: Self::LoadSt) -> Self::UnloadSt {
//         (self.unload)(state)
//     }
//     fn get(&self, state: Self::LoadSt) -> Self::Value {
//         (self.get)(state)
//     }
// }
// pub fn fold_by_op<LoadSt: 'static, UnloadSt: 'static, Value: 'static>(
//     load: impl FnMut(UnloadSt, &mut BindContext) -> LoadSt + 'static,
//     unload: impl FnMut(LoadSt) -> UnloadSt + 'static,
//     get: impl Fn(LoadSt) -> Value + 'static,
// ) -> impl FoldByOp<LoadSt = LoadSt, UnloadSt = UnloadSt, Value = Value> {
//     AnonymousFoldByOp {
//         load,
//         unload,
//         get,
//         get_phatnom: PhantomData,
//     }
// }
// pub fn fold_op<St: 'static>(
//     load: impl FnMut(St, &mut BindContext) -> St + 'static,
// ) -> impl FoldByOp<LoadSt = St, UnloadSt = St, Value = St> {
//     fold_by_op(load, |st| st, |st| st)
// }

// pub struct Scan<Op: ScanOp> {
//     data: RefCell<ScanData<Op>>,
//     sinks: BindSinks,
// }
// impl<Op: ScanOp> Scan<Op> {
//     pub fn new(initial_state: Op::UnloadSt, op: Op) -> Self {
//         Self {
//             data: RefCell::new(ScanData {
//                 op,
//                 state: ScanState::Unloaded(initial_state),
//                 bindings: Bindings::new(),
//             }),
//             sinks: BindSinks::new(),
//         }
//     }
//     fn borrow<'a>(self: &'a Rc<Self>, cx: &mut BindContext) -> Ref<'a, Op::Value> {
//         cx.bind(self.clone());
//         let mut d = self.data.borrow();
//         if !d.state.is_loaded() {
//             drop(d);
//             self.data.borrow_mut().load(cx.scope(), self);
//             d = self.data.borrow();
//         }
//         Ref::map(d, |d| d.get())
//     }
// }
// impl<Op: ScanOp> DynamicObservableInner for Scan<Op> {
//     type Item = Op::Value;
//     fn dyn_with(
//         self: Rc<Self>,
//         f: &mut dyn FnMut(&Self::Item, &mut BindContext),
//         cx: &mut BindContext,
//     ) {
//         f(&self.borrow(cx), cx)
//     }
// }

// impl<Op: ScanOp> BindSource for Scan<Op> {
//     fn sinks(&self) -> &BindSinks {
//         &self.sinks
//     }
//     fn detach_sink(&self, idx: usize) {
//         self.sinks.detach(idx);
//         if self.sinks.is_empty() {
//             let d = &mut *self.data.borrow_mut();
//             d.bindings.clear();
//             d.unload();
//         }
//     }
// }

// impl<Op: ScanOp> BindSink for Scan<Op> {
//     fn notify(self: Rc<Self>, scope: &NotifyScope) {
//         if self.data.borrow_mut().unload() {
//             self.sinks.notify(scope);
//         }
//     }
// }

// pub struct FilterScan<Op: FilterScanOp> {
//     data: RefCell<FilterScanData<Op>>,
//     sinks: BindSinks,
// }

// impl<Op: FilterScanOp> FilterScan<Op> {
//     pub fn new(initial_state: Op::UnloadSt, op: Op) -> Self {
//         Self {
//             data: RefCell::new(FilterScanData {
//                 op,
//                 state: ScanState::Unloaded(initial_state),
//                 bindings: Bindings::new(),
//             }),
//             sinks: BindSinks::new(),
//         }
//     }

//     fn ready(self: &Rc<Self>, scope: &BindScope) {
//         if self.data.borrow_mut().load(scope, self) {
//             scope.defer_notify(self.clone());
//         }
//     }
//     fn borrow<'a>(self: &'a Rc<Self>, cx: &mut BindContext) -> Ref<'a, Op::Value> {
//         let mut d = self.data.borrow();
//         if !d.state.is_loaded() {
//             drop(d);
//             self.ready(cx.scope());
//             d = self.data.borrow();
//         }
//         cx.bind(self.clone());
//         Ref::map(d, |d| d.get())
//     }
// }

// impl<Op: FilterScanOp> DynamicObservableInner for FilterScan<Op> {
//     type Item = Op::Value;
//     fn dyn_with(
//         self: Rc<Self>,
//         f: &mut dyn FnMut(&Self::Item, &mut BindContext),
//         cx: &mut BindContext,
//     ) {
//         f(&self.borrow(cx), cx)
//     }
// }

// impl<Op: FilterScanOp> BindSource for FilterScan<Op> {
//     fn sinks(&self) -> &BindSinks {
//         &self.sinks
//     }
//     fn detach_sink(&self, idx: usize) {
//         self.sinks.detach(idx);
//         if self.sinks.is_empty() {
//             let d = &mut *self.data.borrow_mut();
//             d.bindings.clear();
//             d.unload();
//         }
//     }
// }

// impl<Op: FilterScanOp> BindSink for FilterScan<Op> {
//     fn notify(self: Rc<Self>, scope: &NotifyScope) {
//         if self.data.borrow_mut().unload() && !self.sinks.is_empty() {
//             scope.defer_bind(self);
//         }
//     }
// }
// impl<Op: FilterScanOp> BindTask for FilterScan<Op> {
//     fn run(self: Rc<Self>, scope: &BindScope) {
//         self.ready(scope);
//     }
// }

// pub struct FoldBy<Op: FoldByOp>(RefCell<FoldByData<Op>>);

// impl<Op: FoldByOp> FoldBy<Op> {
//     pub fn new(state: Op::UnloadSt, op: Op) -> Rc<Self> {
//         Self::new_with_state(ScanState::Unloaded(state), op)
//     }
//     pub fn new_with_state(state: ScanState<Op::LoadSt, Op::UnloadSt>, op: Op) -> Rc<Self> {
//         let is_loaded = state.is_loaded();
//         let this = Rc::new(FoldBy(RefCell::new(FoldByData {
//             op,
//             state,
//             bindings: Bindings::new(),
//         })));
//         if !is_loaded {
//             BindScope::with(|scope| Self::next(&this, scope));
//         }
//         this
//     }
//     fn next(this: &Rc<Self>, scope: &BindScope) {
//         this.0.borrow_mut().load(scope, this);
//     }
// }
// impl<Op: FoldByOp> DynamicFold for FoldBy<Op> {
//     type Output = Op::Value;

//     fn stop(self: Rc<Self>, scope: &BindScope) -> Self::Output {
//         let d = &mut *(self.0).borrow_mut();
//         d.load(scope, &self);
//         d.bindings.clear();
//         if let ScanState::Loaded(state) = take(&mut d.state) {
//             d.op.get(state)
//         } else {
//             panic!("invalid state.")
//         }
//     }
//     fn as_dyn_any(self: Rc<Self>) -> Rc<dyn Any> {
//         self
//     }
// }

// impl<Op: FoldByOp> BindSink for FoldBy<Op> {
//     fn notify(self: Rc<Self>, scope: &NotifyScope) {
//         if self.0.borrow_mut().unload() {
//             scope.defer_bind(self);
//         }
//     }
// }

// impl<Op: FoldByOp> BindTask for FoldBy<Op> {
//     fn run(self: Rc<Self>, scope: &BindScope) {
//         Self::next(&self, scope);
//     }
// }
// impl<Op: FoldByOp> Drop for FoldBy<Op> {
//     fn drop(&mut self) {
//         self.0.borrow_mut().unload();
//     }
// }

// pub struct ObserverOp<S, O> {
//     s: S,
//     o: O,
// }
// impl<S, O> ObserverOp<S, O> {
//     pub fn new(s: S, o: O) -> Self {
//         Self { s, o }
//     }
// }
// pub trait AsObserver<O> {
//     fn as_observer(&self) -> &O;
//     fn as_observer_mut(&mut self) -> &mut O;
// }
// impl<S, O> AsObserver<O> for ObserverOp<S, O> {
//     fn as_observer(&self) -> &O {
//         &self.o
//     }
//     fn as_observer_mut(&mut self) -> &mut O {
//         &mut self.o
//     }
// }

// impl<S, O> FoldByOp for ObserverOp<Obs<S>, O>
// where
//     S: Observable,
//     for<'a> O: Observer<&'a S::Item>,
// {
//     type LoadSt = ();
//     type UnloadSt = ();
//     type Value = ();

//     fn load(&mut self, _state: Self::UnloadSt, cx: &mut BindContext) -> Self::LoadSt {
//         let o = &mut self.o;
//         self.s.with(|value, _cx| o.next(value), cx)
//     }
//     fn unload(&mut self, _state: Self::LoadSt) -> Self::UnloadSt {}
//     fn get(&self, _state: Self::LoadSt) -> Self::Value {}
// }

// impl<Op, O> InnerSubscriber<O> for FoldBy<Op>
// where
//     Op: FoldByOp + AsObserver<O>,
// {
//     fn borrow(&self) -> Ref<O> {
//         Ref::map(self.0.borrow(), |x| x.op.as_observer())
//     }
//     fn borrow_mut(&self) -> RefMut<O> {
//         RefMut::map(self.0.borrow_mut(), |x| x.op.as_observer_mut())
//     }
//     fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any> {
//         self
//     }
// }
