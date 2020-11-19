use std::{
    any::Any,
    cell::{Ref, RefCell},
    marker::PhantomData,
    mem::take,
    rc::Rc,
};

use crate::{
    BindContext, BindScope, BindSink, BindSinks, BindSource, BindTask, Bindings, NotifyScope,
    ReactiveBorrow,
};

use super::{DynamicFold, DynamicReactiveBorrowSource, DynamicReactiveRefSource, DynamicTask};

pub trait ScanOp: 'static {
    type LoadSt;
    type UnloadSt;
    type Value;
    fn load(&mut self, state: Self::UnloadSt, ctx: &BindContext) -> Self::LoadSt;
    fn unload(&mut self, state: Self::LoadSt) -> Self::UnloadSt;
    fn get<'a>(&self, state: &'a Self::LoadSt) -> &'a Self::Value;
}
pub trait FilterScanOp: 'static {
    type LoadSt;
    type UnloadSt;
    type Value;
    fn load(&mut self, state: Self::UnloadSt, ctx: &BindContext) -> FilterScanLoad<Self::LoadSt>;
    fn unload(&mut self, state: Self::LoadSt) -> Self::UnloadSt;
    fn get<'a>(&self, state: &'a Self::LoadSt) -> &'a Self::Value;
}
pub trait FoldByOp: 'static {
    type LoadSt;
    type UnloadSt;
    type Value;
    fn load(&mut self, state: Self::UnloadSt, ctx: &BindContext) -> Self::LoadSt;
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
                *self = Self::Loaded(bindings.update(scope, sink, |ctx| load(state, ctx)));
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
            .load(&mut self.bindings, scope, sink, |state, ctx| {
                op.load(state, ctx)
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
            .load(&mut self.bindings, scope, sink, |state, ctx| {
                let r = op.load(state, ctx);
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
            .load(&mut self.bindings, scope, sink, |state, ctx| {
                op.load(state, ctx)
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

    fn load(&mut self, state: Self::UnloadSt, ctx: &BindContext) -> Self::LoadSt {
        (self.load)(state, ctx)
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

    fn load(&mut self, state: Self::UnloadSt, ctx: &BindContext) -> FilterScanLoad<Self::LoadSt> {
        (self.load)(state, ctx)
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

    fn load(&mut self, state: Self::UnloadSt, ctx: &BindContext) -> Self::LoadSt {
        (self.load)(state, ctx)
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
    fn borrow<'a>(self: &'a Rc<Self>, ctx: &BindContext<'a>) -> Ref<'a, Op::Value> {
        ctx.bind(self.clone());
        let mut d = self.data.borrow();
        if !d.state.is_loaded() {
            drop(d);
            self.data.borrow_mut().load(ctx.scope(), self);
            d = self.data.borrow();
        }
        Ref::map(d, |d| d.get())
    }
}
impl<Op: ScanOp> ReactiveBorrow for Rc<Scan<Op>> {
    type Item = Op::Value;
    fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.borrow(ctx)
    }
}

impl<Op: ScanOp> DynamicReactiveBorrowSource for Scan<Op> {
    type Item = Op::Value;

    fn dyn_borrow(
        &self,
        rc_self: &Rc<dyn DynamicReactiveBorrowSource<Item = Self::Item>>,
        ctx: &BindContext,
    ) -> Ref<Self::Item> {
        let rc_self = Self::downcast(rc_self);
        ctx.bind(rc_self.clone());
        let mut d = self.data.borrow();
        if !d.state.is_loaded() {
            drop(d);
            self.data.borrow_mut().load(ctx.scope(), &rc_self);
            d = self.data.borrow();
        }
        Ref::map(d, |d| d.get())
    }
    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicReactiveRefSource<Item = Self::Item>> {
        self
    }
}

impl<Op: ScanOp> DynamicReactiveRefSource for Scan<Op> {
    type Item = Op::Value;
    fn dyn_with(self: Rc<Self>, f: &mut dyn FnMut(&Self::Item, &BindContext), ctx: &BindContext) {
        f(&self.borrow(ctx), ctx)
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
            NotifyScope::update(self);
        }
    }
    fn borrow<'a>(self: &'a Rc<Self>, ctx: &BindContext<'a>) -> Ref<'a, Op::Value> {
        let mut d = self.data.borrow();
        if !d.state.is_loaded() {
            drop(d);
            self.ready(ctx.scope());
            d = self.data.borrow();
        }
        ctx.bind(self.clone());
        Ref::map(d, |d| d.get())
    }
}

impl<Op: FilterScanOp> ReactiveBorrow for Rc<FilterScan<Op>> {
    type Item = Op::Value;

    fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.borrow(ctx)
    }
}

impl<Op: FilterScanOp> DynamicReactiveBorrowSource for FilterScan<Op> {
    type Item = Op::Value;

    fn dyn_borrow(
        &self,
        rc_self: &Rc<dyn DynamicReactiveBorrowSource<Item = Self::Item>>,
        ctx: &BindContext,
    ) -> Ref<Self::Item> {
        let rc_self = Self::downcast(rc_self);
        let mut d = self.data.borrow();
        if !d.state.is_loaded() {
            drop(d);
            rc_self.ready(ctx.scope());
            d = self.data.borrow();
        }
        ctx.bind(rc_self);
        Ref::map(d, |d| d.get())
    }
    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicReactiveRefSource<Item = Self::Item>> {
        self
    }
}
impl<Op: FilterScanOp> DynamicReactiveRefSource for FilterScan<Op> {
    type Item = Op::Value;
    fn dyn_with(self: Rc<Self>, f: &mut dyn FnMut(&Self::Item, &BindContext), ctx: &BindContext) {
        f(&self.borrow(ctx), ctx)
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
            scope.spawn(self);
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
impl<Op: FoldByOp> DynamicTask for FoldBy<Op> {
    fn as_dyn_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}

impl<Op: FoldByOp> BindSink for FoldBy<Op> {
    fn notify(self: Rc<Self>, scope: &NotifyScope) {
        if self.0.borrow_mut().unload() {
            scope.spawn(self);
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
