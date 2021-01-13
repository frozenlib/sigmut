use crate::*;
use std::{
    any::Any,
    cell::{Ref, RefCell, RefMut},
    marker::PhantomData,
    mem::take,
    rc::Rc,
};

use super::{DynamicFold, DynamicObservableBorrowSource, DynamicObservableRefSource};

pub trait ScanOp: 'static {
    type LoadSt;
    type UnloadSt;
    type Value;
    fn load(&mut self, state: Self::UnloadSt, cx: &BindContext) -> Self::LoadSt;
    fn unload(&mut self, state: Self::LoadSt) -> Self::UnloadSt;
    fn get<'a>(&self, state: &'a Self::LoadSt) -> &'a Self::Value;
}
pub trait FilterScanOp: 'static {
    type LoadSt;
    type UnloadSt;
    type Value;
    fn load(&mut self, state: Self::UnloadSt, cx: &BindContext) -> FilterScanLoad<Self::LoadSt>;
    fn unload(&mut self, state: Self::LoadSt) -> Self::UnloadSt;
    fn get<'a>(&self, state: &'a Self::LoadSt) -> &'a Self::Value;
}
pub trait FoldByOp: 'static {
    type LoadSt;
    type UnloadSt;
    type Value;
    fn load(&mut self, state: Self::UnloadSt, cx: &BindContext) -> Self::LoadSt;
    fn unload(&mut self, state: Self::LoadSt) -> Self::UnloadSt;
    fn get(&self, state: Self::LoadSt) -> Self::Value;
}

pub struct FilterScanLoad<LoadSt> {
    pub state: LoadSt,
    pub is_notify: bool,
}
struct ScanData<Op: ScanOp> {
    op: Op,
    state: ScanState<Op::LoadSt, Op::UnloadSt>,
    bindings: Bindings,
}
struct FilterScanData<Op: FilterScanOp> {
    op: Op,
    state: ScanState<Op::LoadSt, Op::UnloadSt>,
    bindings: Bindings,
}
struct FoldByData<Op: FoldByOp> {
    op: Op,
    state: ScanState<Op::LoadSt, Op::UnloadSt>,
    bindings: Bindings,
}

pub enum ScanState<LoadSt, UnloadSt> {
    NoData,
    Loaded(LoadSt),
    Unloaded(UnloadSt),
}
impl<LoadSt, UnloadSt> Default for ScanState<LoadSt, UnloadSt> {
    fn default() -> Self {
        Self::NoData
    }
}

impl<LoadSt, UnloadSt> ScanState<LoadSt, UnloadSt> {
    fn load(
        &mut self,
        bindings: &mut Bindings,
        scope: &BindScope,
        sink: &Rc<impl BindSink>,
        load: impl FnOnce(UnloadSt, &BindContext) -> LoadSt,
    ) -> bool {
        if let Self::Unloaded(_) = self {
            if let Self::Unloaded(state) = take(self) {
                *self = Self::Loaded(bindings.update(scope, sink, |cx| load(state, cx)));
                return true;
            } else {
                unreachable!()
            }
        }
        false
    }

    fn unload(&mut self, unload: impl FnOnce(LoadSt) -> UnloadSt) -> bool {
        if let Self::Loaded(_) = self {
            if let Self::Loaded(value) = take(self) {
                *self = Self::Unloaded(unload(value));
                return true;
            } else {
                unreachable!()
            }
        }
        false
    }
    fn is_loaded(&self) -> bool {
        match self {
            Self::Loaded(_) => true,
            Self::Unloaded(_) => false,
            Self::NoData => panic!("ScanState invalid state."),
        }
    }
    fn get<'a, T>(&'a self, get: impl Fn(&'a LoadSt) -> &'a T) -> &'a T {
        if let ScanState::Loaded(state) = &self {
            get(state)
        } else {
            panic!("value not loaded.")
        }
    }
}

impl<Op: ScanOp> ScanData<Op> {
    fn load(&mut self, scope: &BindScope, sink: &Rc<impl BindSink>) -> bool {
        let op = &mut self.op;
        self.state
            .load(&mut self.bindings, scope, sink, |state, cx| {
                op.load(state, cx)
            })
    }
    fn unload(&mut self) -> bool {
        let op = &mut self.op;
        self.state.unload(|state| op.unload(state))
    }
    fn get(&self) -> &Op::Value {
        self.state.get(|state| self.op.get(state))
    }
}
impl<Op: FilterScanOp> FilterScanData<Op> {
    fn load(&mut self, scope: &BindScope, sink: &Rc<impl BindSink>) -> bool {
        let mut is_notify = false;
        let op = &mut self.op;
        self.state
            .load(&mut self.bindings, scope, sink, |state, cx| {
                let r = op.load(state, cx);
                is_notify = r.is_notify;
                r.state
            });
        is_notify
    }
    fn unload(&mut self) -> bool {
        let op = &mut self.op;
        self.state.unload(|state| op.unload(state))
    }
    fn get(&self) -> &Op::Value {
        self.state.get(|state| self.op.get(state))
    }
}
impl<Op: FoldByOp> FoldByData<Op> {
    fn load(&mut self, scope: &BindScope, sink: &Rc<impl BindSink>) -> bool {
        let op = &mut self.op;
        self.state
            .load(&mut self.bindings, scope, sink, |state, cx| {
                op.load(state, cx)
            })
    }
    fn unload(&mut self) -> bool {
        let op = &mut self.op;
        self.state.unload(|state| op.unload(state))
    }
}

struct AnonymousScanOp<LoadSt, UnloadSt, Value, Load, Unload, Get>
where
    Load: FnMut(UnloadSt, &BindContext) -> LoadSt,
    Unload: FnMut(LoadSt) -> UnloadSt,
    Get: Fn(&LoadSt) -> &Value,
{
    load: Load,
    unload: Unload,
    get: Get,
    get_phatnom: PhantomData<fn(&LoadSt) -> &Value>,
}
impl<LoadSt, UnloadSt, Value, Load, Unload, Get> ScanOp
    for AnonymousScanOp<LoadSt, UnloadSt, Value, Load, Unload, Get>
where
    Load: FnMut(UnloadSt, &BindContext) -> LoadSt + 'static,
    Unload: FnMut(LoadSt) -> UnloadSt + 'static,
    Get: Fn(&LoadSt) -> &Value + 'static,
    LoadSt: 'static,
    UnloadSt: 'static,
    Value: 'static,
{
    type LoadSt = LoadSt;
    type UnloadSt = UnloadSt;
    type Value = Value;

    fn load(&mut self, state: Self::UnloadSt, cx: &BindContext) -> Self::LoadSt {
        (self.load)(state, cx)
    }
    fn unload(&mut self, state: Self::LoadSt) -> Self::UnloadSt {
        (self.unload)(state)
    }
    fn get<'a>(&self, state: &'a Self::LoadSt) -> &'a Self::Value {
        (self.get)(state)
    }
}
pub fn scan_op<LoadSt: 'static, UnloadSt: 'static, Value: 'static>(
    load: impl FnMut(UnloadSt, &BindContext) -> LoadSt + 'static,
    unload: impl FnMut(LoadSt) -> UnloadSt + 'static,
    get: impl Fn(&LoadSt) -> &Value + 'static,
) -> impl ScanOp<LoadSt = LoadSt, UnloadSt = UnloadSt, Value = Value> {
    AnonymousScanOp {
        load,
        unload,
        get,
        get_phatnom: PhantomData,
    }
}

struct AnonymousFilterScanOp<LoadSt, UnloadSt, Value, Load, Unload, Get>
where
    Load: FnMut(UnloadSt, &BindContext) -> FilterScanLoad<LoadSt>,
    Unload: FnMut(LoadSt) -> UnloadSt,
    Get: Fn(&LoadSt) -> &Value,
{
    load: Load,
    unload: Unload,
    get: Get,
    get_phatnom: PhantomData<fn(&LoadSt) -> &Value>,
}
impl<LoadSt, UnloadSt, Value, Load, Unload, Get> FilterScanOp
    for AnonymousFilterScanOp<LoadSt, UnloadSt, Value, Load, Unload, Get>
where
    Load: FnMut(UnloadSt, &BindContext) -> FilterScanLoad<LoadSt> + 'static,
    Unload: FnMut(LoadSt) -> UnloadSt + 'static,
    Get: Fn(&LoadSt) -> &Value + 'static,
    LoadSt: 'static,
    UnloadSt: 'static,
    Value: 'static,
{
    type LoadSt = LoadSt;
    type UnloadSt = UnloadSt;
    type Value = Value;

    fn load(&mut self, state: Self::UnloadSt, cx: &BindContext) -> FilterScanLoad<Self::LoadSt> {
        (self.load)(state, cx)
    }
    fn unload(&mut self, state: Self::LoadSt) -> Self::UnloadSt {
        (self.unload)(state)
    }
    fn get<'a>(&self, state: &'a Self::LoadSt) -> &'a Self::Value {
        (self.get)(state)
    }
}
pub fn filter_scan_op<LoadSt: 'static, UnloadSt: 'static, Value: 'static>(
    load: impl FnMut(UnloadSt, &BindContext) -> FilterScanLoad<LoadSt> + 'static,
    unload: impl FnMut(LoadSt) -> UnloadSt + 'static,
    get: impl Fn(&LoadSt) -> &Value + 'static,
) -> impl FilterScanOp<LoadSt = LoadSt, UnloadSt = UnloadSt, Value = Value> {
    AnonymousFilterScanOp {
        load,
        unload,
        get,
        get_phatnom: PhantomData,
    }
}

struct AnonymousFoldByOp<LoadSt, UnloadSt, Value, Load, Unload, Get>
where
    Load: FnMut(UnloadSt, &BindContext) -> LoadSt,
    Unload: FnMut(LoadSt) -> UnloadSt,
    Get: Fn(LoadSt) -> Value,
{
    load: Load,
    unload: Unload,
    get: Get,
    get_phatnom: PhantomData<fn(&LoadSt) -> &Value>,
}
impl<LoadSt, UnloadSt, Value, Load, Unload, Get> FoldByOp
    for AnonymousFoldByOp<LoadSt, UnloadSt, Value, Load, Unload, Get>
where
    Load: FnMut(UnloadSt, &BindContext) -> LoadSt + 'static,
    Unload: FnMut(LoadSt) -> UnloadSt + 'static,
    Get: Fn(LoadSt) -> Value + 'static,
    LoadSt: 'static,
    UnloadSt: 'static,
    Value: 'static,
{
    type LoadSt = LoadSt;
    type UnloadSt = UnloadSt;
    type Value = Value;

    fn load(&mut self, state: Self::UnloadSt, cx: &BindContext) -> Self::LoadSt {
        (self.load)(state, cx)
    }
    fn unload(&mut self, state: Self::LoadSt) -> Self::UnloadSt {
        (self.unload)(state)
    }
    fn get(&self, state: Self::LoadSt) -> Self::Value {
        (self.get)(state)
    }
}
pub fn fold_by_op<LoadSt: 'static, UnloadSt: 'static, Value: 'static>(
    load: impl FnMut(UnloadSt, &BindContext) -> LoadSt + 'static,
    unload: impl FnMut(LoadSt) -> UnloadSt + 'static,
    get: impl Fn(LoadSt) -> Value + 'static,
) -> impl FoldByOp<LoadSt = LoadSt, UnloadSt = UnloadSt, Value = Value> {
    AnonymousFoldByOp {
        load,
        unload,
        get,
        get_phatnom: PhantomData,
    }
}

pub struct Scan<Op: ScanOp> {
    data: RefCell<ScanData<Op>>,
    sinks: BindSinks,
}
impl<Op: ScanOp> Scan<Op> {
    pub fn new(initial_state: Op::UnloadSt, op: Op) -> Self {
        Self {
            data: RefCell::new(ScanData {
                op,
                state: ScanState::Unloaded(initial_state),
                bindings: Bindings::new(),
            }),
            sinks: BindSinks::new(),
        }
    }
    fn borrow<'a>(self: &'a Rc<Self>, cx: &BindContext<'a>) -> Ref<'a, Op::Value> {
        cx.bind(self.clone());
        let mut d = self.data.borrow();
        if !d.state.is_loaded() {
            drop(d);
            self.data.borrow_mut().load(cx.scope(), self);
            d = self.data.borrow();
        }
        Ref::map(d, |d| d.get())
    }
}
impl<Op: ScanOp> ObservableBorrow for Rc<Scan<Op>> {
    type Item = Op::Value;
    fn borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.borrow(cx)
    }
}

impl<Op: ScanOp> DynamicObservableBorrowSource for Scan<Op> {
    type Item = Op::Value;

    fn dyn_borrow<'a>(
        &'a self,
        rc_self: &Rc<dyn DynamicObservableBorrowSource<Item = Self::Item>>,
        cx: &BindContext<'a>,
    ) -> Ref<'a, Self::Item> {
        let rc_self = Self::downcast(rc_self);
        cx.bind(rc_self.clone());
        let mut d = self.data.borrow();
        if !d.state.is_loaded() {
            drop(d);
            self.data.borrow_mut().load(cx.scope(), &rc_self);
            d = self.data.borrow();
        }
        Ref::map(d, |d| d.get())
    }
    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRefSource<Item = Self::Item>> {
        self
    }
}

impl<Op: ScanOp> DynamicObservableRefSource for Scan<Op> {
    type Item = Op::Value;
    fn dyn_with(self: Rc<Self>, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
        f(&self.borrow(cx), cx)
    }
}

impl<Op: ScanOp> BindSource for Scan<Op> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
    fn detach_sink(&self, idx: usize) {
        self.sinks.detach(idx);
        if self.sinks.is_empty() {
            let d = &mut *self.data.borrow_mut();
            d.bindings.clear();
            d.unload();
        }
    }
}

impl<Op: ScanOp> BindSink for Scan<Op> {
    fn notify(self: Rc<Self>, scope: &NotifyScope) {
        if self.data.borrow_mut().unload() {
            self.sinks.notify(scope);
        }
    }
}

pub struct FilterScan<Op: FilterScanOp> {
    data: RefCell<FilterScanData<Op>>,
    sinks: BindSinks,
}

impl<Op: FilterScanOp> FilterScan<Op> {
    pub fn new(initial_state: Op::UnloadSt, op: Op) -> Self {
        Self {
            data: RefCell::new(FilterScanData {
                op,
                state: ScanState::Unloaded(initial_state),
                bindings: Bindings::new(),
            }),
            sinks: BindSinks::new(),
        }
    }

    fn ready(self: &Rc<Self>, scope: &BindScope) {
        if self.data.borrow_mut().load(scope, self) {
            scope.defer_notify(self.clone());
        }
    }
    fn borrow<'a>(self: &'a Rc<Self>, cx: &BindContext<'a>) -> Ref<'a, Op::Value> {
        let mut d = self.data.borrow();
        if !d.state.is_loaded() {
            drop(d);
            self.ready(cx.scope());
            d = self.data.borrow();
        }
        cx.bind(self.clone());
        Ref::map(d, |d| d.get())
    }
}

impl<Op: FilterScanOp> ObservableBorrow for Rc<FilterScan<Op>> {
    type Item = Op::Value;

    fn borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.borrow(cx)
    }
}

impl<Op: FilterScanOp> DynamicObservableBorrowSource for FilterScan<Op> {
    type Item = Op::Value;

    fn dyn_borrow<'a>(
        &'a self,
        rc_self: &Rc<dyn DynamicObservableBorrowSource<Item = Self::Item>>,
        cx: &BindContext<'a>,
    ) -> Ref<'a, Self::Item> {
        let rc_self = Self::downcast(rc_self);
        let mut d = self.data.borrow();
        if !d.state.is_loaded() {
            drop(d);
            rc_self.ready(cx.scope());
            d = self.data.borrow();
        }
        cx.bind(rc_self);
        Ref::map(d, |d| d.get())
    }
    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRefSource<Item = Self::Item>> {
        self
    }
}
impl<Op: FilterScanOp> DynamicObservableRefSource for FilterScan<Op> {
    type Item = Op::Value;
    fn dyn_with(self: Rc<Self>, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
        f(&self.borrow(cx), cx)
    }
}

impl<Op: FilterScanOp> BindSource for FilterScan<Op> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
    fn detach_sink(&self, idx: usize) {
        self.sinks.detach(idx);
        if self.sinks.is_empty() {
            let d = &mut *self.data.borrow_mut();
            d.bindings.clear();
            d.unload();
        }
    }
}

impl<Op: FilterScanOp> BindSink for FilterScan<Op> {
    fn notify(self: Rc<Self>, scope: &NotifyScope) {
        if self.data.borrow_mut().unload() && !self.sinks.is_empty() {
            scope.defer_bind(self);
        }
    }
}
impl<Op: FilterScanOp> BindTask for FilterScan<Op> {
    fn run(self: Rc<Self>, scope: &BindScope) {
        self.ready(scope);
    }
}

pub struct FoldBy<Op: FoldByOp>(RefCell<FoldByData<Op>>);

impl<Op: FoldByOp> FoldBy<Op> {
    pub fn new(state: Op::UnloadSt, op: Op) -> Rc<Self> {
        Self::new_with_state(ScanState::Unloaded(state), op)
    }
    pub fn new_with_state(state: ScanState<Op::LoadSt, Op::UnloadSt>, op: Op) -> Rc<Self> {
        let is_loaded = state.is_loaded();
        let this = Rc::new(FoldBy(RefCell::new(FoldByData {
            op,
            state,
            bindings: Bindings::new(),
        })));
        if !is_loaded {
            BindScope::with(|scope| Self::next(&this, scope));
        }
        this
    }
    fn next(this: &Rc<Self>, scope: &BindScope) {
        this.0.borrow_mut().load(scope, this);
    }
}
impl<Op: FoldByOp> DynamicFold for FoldBy<Op> {
    type Output = Op::Value;

    fn stop(self: Rc<Self>, scope: &BindScope) -> Self::Output {
        let d = &mut *(self.0).borrow_mut();
        d.load(scope, &self);
        d.bindings.clear();
        if let ScanState::Loaded(state) = take(&mut d.state) {
            d.op.get(state)
        } else {
            panic!("invalid state.")
        }
    }
    fn as_dyn_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}

impl<Op: FoldByOp> BindSink for FoldBy<Op> {
    fn notify(self: Rc<Self>, scope: &NotifyScope) {
        if self.0.borrow_mut().unload() {
            scope.defer_bind(self);
        }
    }
}

impl<Op: FoldByOp> BindTask for FoldBy<Op> {
    fn run(self: Rc<Self>, scope: &BindScope) {
        Self::next(&self, scope);
    }
}
impl<Op: FoldByOp> Drop for FoldBy<Op> {
    fn drop(&mut self) {
        self.0.borrow_mut().unload();
    }
}

pub trait Subscriber<O> {
    fn borrow(&self) -> Ref<O>;
    fn borrow_mut(&self) -> RefMut<O>;
    fn as_dyn(&self) -> DynSubscriber<O>;
    fn as_subscription(&self) -> Subscription;
}

pub struct DynSubscriber<O>(Rc<dyn InnerSubscriber<O>>);

impl<O: 'static> DynSubscriber<O> {
    pub fn borrow(&self) -> Ref<O> {
        self.0.borrow()
    }
    pub fn borrow_mut(&self) -> RefMut<O> {
        self.0.borrow_mut()
    }
}
impl<O: 'static> From<DynSubscriber<O>> for Subscription {
    fn from(s: DynSubscriber<O>) -> Self {
        Self(Some(s.0.as_any()))
    }
}

trait InnerSubscriber<O>: 'static {
    fn borrow(&self) -> Ref<O>;
    fn borrow_mut(&self) -> RefMut<O>;
    fn as_any(self: Rc<Self>) -> Rc<dyn Any>;
}
struct OuterSubscriber<I>(Rc<I>);

impl<I: InnerSubscriber<O>, O: 'static> Subscriber<O> for OuterSubscriber<I> {
    fn borrow(&self) -> Ref<O> {
        self.0.borrow()
    }
    fn borrow_mut(&self) -> RefMut<O> {
        self.0.borrow_mut()
    }
    fn as_dyn(&self) -> DynSubscriber<O> {
        DynSubscriber(self.0.clone())
    }
    fn as_subscription(&self) -> Subscription {
        self.as_dyn().into()
    }
}

pub(crate) fn subscribe_value<S, O>(s: Obs<S>, o: O) -> impl Subscriber<O>
where
    S: Observable,
    O: Observer<S::Item>,
{
    OuterSubscriber(FoldBy::new((), ObserverOp { s, o }))
}
pub(crate) fn subscribe_ref<S, O>(s: ObsRef<S>, o: O) -> impl Subscriber<O>
where
    S: ObservableRef,
    for<'a> O: Observer<&'a S::Item>,
{
    OuterSubscriber(FoldBy::new((), ObserverOp { s, o }))
}

struct ObserverOp<S, O> {
    s: S,
    o: O,
}
impl<S, O> FoldByOp for ObserverOp<Obs<S>, O>
where
    S: Observable,
    O: Observer<S::Item>,
{
    type LoadSt = ();
    type UnloadSt = ();
    type Value = ();

    fn load(&mut self, _state: Self::UnloadSt, cx: &BindContext) -> Self::LoadSt {
        self.o.next(self.s.get(cx))
    }
    fn unload(&mut self, _state: Self::LoadSt) -> Self::UnloadSt {}
    fn get(&self, _state: Self::LoadSt) -> Self::Value {}
}

impl<S, O> FoldByOp for ObserverOp<ObsRef<S>, O>
where
    S: ObservableRef,
    for<'a> O: Observer<&'a S::Item>,
{
    type LoadSt = ();
    type UnloadSt = ();
    type Value = ();

    fn load(&mut self, _state: Self::UnloadSt, cx: &BindContext) -> Self::LoadSt {
        let o = &mut self.o;
        self.s.with(|value, _cx| o.next(value), cx)
    }
    fn unload(&mut self, _state: Self::LoadSt) -> Self::UnloadSt {}
    fn get(&self, _state: Self::LoadSt) -> Self::Value {}
}

impl<S, O> InnerSubscriber<O> for FoldBy<ObserverOp<S, O>>
where
    ObserverOp<S, O>: FoldByOp,
{
    fn borrow(&self) -> Ref<O> {
        Ref::map(self.0.borrow(), |x| &x.op.o)
    }
    fn borrow_mut(&self) -> RefMut<O> {
        RefMut::map(self.0.borrow_mut(), |x| &mut x.op.o)
    }
    fn as_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}
