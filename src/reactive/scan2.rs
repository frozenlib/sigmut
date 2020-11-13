use std::{
    any::Any,
    cell::{Ref, RefCell},
    marker::PhantomData,
    mem::take,
    rc::Rc,
};

use crate::{
    BindContext, BindContextScope, BindSink, BindSinks, BindSource, BindTask, Bindings,
    NotifyContext, ReactiveBorrow,
};

use super::{DynamicFold, DynamicReactiveBorrowSource, DynamicReactiveRefSource};

pub trait ScanSchema: 'static {
    type LoadSt;
    type UnloadSt;
    type Value;
    fn load(&self, state: Self::UnloadSt, ctx: &BindContext) -> Self::LoadSt;
    fn unload(&self, state: Self::LoadSt) -> Self::UnloadSt;
    fn get<'a>(&self, state: &'a Self::LoadSt) -> &'a Self::Value;
}
pub trait FilterScanSchema: 'static {
    type LoadSt;
    type UnloadSt;
    type Value;
    fn load(&self, state: Self::UnloadSt, ctx: &BindContext) -> FilterScanLoad<Self::LoadSt>;
    fn unload(&self, state: Self::LoadSt) -> Self::UnloadSt;
    fn get<'a>(&self, state: &'a Self::LoadSt) -> &'a Self::Value;
}
pub trait FoldBySchema: 'static {
    type LoadSt;
    type UnloadSt;
    type Value;
    fn load(&self, state: Self::UnloadSt, ctx: &BindContext) -> Self::LoadSt;
    fn unload(&self, state: Self::LoadSt) -> Self::UnloadSt;
    fn get(&self, state: Self::LoadSt) -> Self::Value;
}

pub struct FilterScanLoad<LoadSt> {
    state: LoadSt,
    is_notify: bool,
}
struct ScanData<S: ScanSchema> {
    schema: S,
    state: ScanState<S::LoadSt, S::UnloadSt>,
    bindings: Bindings,
}
struct FilterScanData<S: FilterScanSchema> {
    schema: S,
    state: ScanState<S::LoadSt, S::UnloadSt>,
    bindings: Bindings,
}
struct FoldByData<S: FoldBySchema> {
    schema: S,
    state: ScanState<S::LoadSt, S::UnloadSt>,
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
        scope: &BindContextScope,
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

impl<S: ScanSchema> ScanData<S> {
    fn load(&mut self, scope: &BindContextScope, sink: &Rc<impl BindSink>) -> bool {
        let schema = &mut self.schema;
        self.state
            .load(&mut self.bindings, scope, sink, |state, ctx| {
                schema.load(state, ctx)
            })
    }
    fn unload(&mut self) -> bool {
        let schema = &self.schema;
        self.state.unload(|state| schema.unload(state))
    }
    fn get(&self) -> &S::Value {
        self.state.get(|state| self.schema.get(state))
    }
}
impl<S: FilterScanSchema> FilterScanData<S> {
    fn load(&mut self, scope: &BindContextScope, sink: &Rc<impl BindSink>) -> bool {
        let mut is_notify = false;
        let schema = &mut self.schema;
        self.state
            .load(&mut self.bindings, scope, sink, |state, ctx| {
                let r = schema.load(state, ctx);
                is_notify = r.is_notify;
                r.state
            });
        is_notify
    }
    fn unload(&mut self) -> bool {
        let schema = &self.schema;
        self.state.unload(|state| schema.unload(state))
    }
    fn get(&self) -> &S::Value {
        self.state.get(|state| self.schema.get(state))
    }
}
impl<S: FoldBySchema> FoldByData<S> {
    fn load(&mut self, scope: &BindContextScope, sink: &Rc<impl BindSink>) -> bool {
        let schema = &mut self.schema;
        self.state
            .load(&mut self.bindings, scope, sink, |state, ctx| {
                schema.load(state, ctx)
            })
    }
    fn unload(&mut self) -> bool {
        let schema = &self.schema;
        self.state.unload(|state| schema.unload(state))
    }
}

pub struct Scan<S: ScanSchema> {
    data: RefCell<ScanData<S>>,
    sinks: BindSinks,
}

struct AnonymousScanSchema<LoadSt, UnloadSt, Value, Load, Unload, Get>
where
    Load: Fn(UnloadSt, &BindContext) -> LoadSt,
    Unload: Fn(LoadSt) -> UnloadSt,
    Get: Fn(&LoadSt) -> &Value,
{
    load: Load,
    unload: Unload,
    get: Get,
    get_phatnom: PhantomData<fn(&LoadSt) -> &Value>,
}
impl<LoadSt, UnloadSt, Value, Load, Unload, Get> ScanSchema
    for AnonymousScanSchema<LoadSt, UnloadSt, Value, Load, Unload, Get>
where
    Load: Fn(UnloadSt, &BindContext) -> LoadSt + 'static,
    Unload: Fn(LoadSt) -> UnloadSt + 'static,
    Get: Fn(&LoadSt) -> &Value + 'static,
    LoadSt: 'static,
    UnloadSt: 'static,
    Value: 'static,
{
    type LoadSt = LoadSt;
    type UnloadSt = UnloadSt;
    type Value = Value;

    fn load(&self, state: Self::UnloadSt, ctx: &BindContext) -> Self::LoadSt {
        (self.load)(state, ctx)
    }

    fn unload(&self, state: Self::LoadSt) -> Self::UnloadSt {
        (self.unload)(state)
    }

    fn get<'a>(&self, state: &'a Self::LoadSt) -> &'a Self::Value {
        (self.get)(state)
    }
}
pub fn scan_schema<LoadSt: 'static, UnloadSt: 'static, Value: 'static>(
    load: impl Fn(UnloadSt, &BindContext) -> LoadSt + 'static,
    unload: impl Fn(LoadSt) -> UnloadSt + 'static,
    get: impl Fn(&LoadSt) -> &Value + 'static,
) -> impl ScanSchema<LoadSt = LoadSt, UnloadSt = UnloadSt, Value = Value> {
    AnonymousScanSchema {
        load,
        unload,
        get,
        get_phatnom: PhantomData,
    }
}

impl<S: ScanSchema> Scan<S> {
    pub fn new(schema: S, initial_state: S::UnloadSt) -> Self {
        Self {
            data: RefCell::new(ScanData {
                schema,
                state: ScanState::Unloaded(initial_state),
                bindings: Bindings::new(),
            }),
            sinks: BindSinks::new(),
        }
    }
    fn borrow<'a>(self: &'a Rc<Self>, ctx: &BindContext<'a>) -> Ref<'a, S::Value> {
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
impl<S: ScanSchema> ReactiveBorrow for Rc<Scan<S>> {
    type Item = S::Value;
    fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.borrow(ctx)
    }
}

impl<S: ScanSchema> DynamicReactiveBorrowSource for Scan<S> {
    type Item = S::Value;

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

impl<S: ScanSchema> DynamicReactiveRefSource for Scan<S> {
    type Item = S::Value;
    fn dyn_with(self: Rc<Self>, ctx: &BindContext, f: &mut dyn FnMut(&BindContext, &Self::Item)) {
        f(ctx, &self.borrow(ctx))
    }
}

impl<S: ScanSchema> BindSource for Scan<S> {
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

impl<S: ScanSchema> BindSink for Scan<S> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        if self.data.borrow_mut().unload() {
            self.sinks.notify(ctx);
        }
    }
}

pub struct FilterScan<S: FilterScanSchema> {
    data: RefCell<FilterScanData<S>>,
    sinks: BindSinks,
}

impl<S: FilterScanSchema> FilterScan<S> {
    pub fn new(schema: S, initial_state: S::UnloadSt) -> Self {
        Self {
            data: RefCell::new(FilterScanData {
                schema,
                state: ScanState::Unloaded(initial_state),
                bindings: Bindings::new(),
            }),
            sinks: BindSinks::new(),
        }
    }

    fn ready(self: &Rc<Self>, scope: &BindContextScope) {
        if self.data.borrow_mut().load(scope, self) {
            NotifyContext::update(self);
        }
    }
    fn borrow<'a>(self: &'a Rc<Self>, ctx: &BindContext<'a>) -> Ref<'a, S::Value> {
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

impl<S: FilterScanSchema> ReactiveBorrow for Rc<FilterScan<S>> {
    type Item = S::Value;

    fn borrow<'a>(&'a self, ctx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.borrow(ctx)
    }
}

impl<S: FilterScanSchema> DynamicReactiveBorrowSource for FilterScan<S> {
    type Item = S::Value;

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
impl<S: FilterScanSchema> DynamicReactiveRefSource for FilterScan<S> {
    type Item = S::Value;
    fn dyn_with(self: Rc<Self>, ctx: &BindContext, f: &mut dyn FnMut(&BindContext, &Self::Item)) {
        f(ctx, &self.borrow(ctx))
    }
}

impl<S: FilterScanSchema> BindSource for FilterScan<S> {
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

impl<S: FilterScanSchema> BindSink for FilterScan<S> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        if self.data.borrow_mut().unload() && !self.sinks.is_empty() {
            ctx.spawn(self);
        }
    }
}
impl<S: FilterScanSchema> BindTask for FilterScan<S> {
    fn run(self: Rc<Self>, scope: &BindContextScope) {
        self.ready(scope);
    }
}

pub struct FoldBy<S: FoldBySchema>(RefCell<FoldByData<S>>);

impl<S: FoldBySchema> FoldBy<S> {
    pub fn new(schema: S, state: ScanState<S::LoadSt, S::UnloadSt>) -> Rc<Self> {
        let is_loaded = state.is_loaded();
        let this = Rc::new(FoldBy(RefCell::new(FoldByData {
            schema,
            state,
            bindings: Bindings::new(),
        })));
        if !is_loaded {
            BindContextScope::with(|scope| Self::next(&this, scope));
        }
        this
    }
    fn next(this: &Rc<Self>, scope: &BindContextScope) {
        this.0.borrow_mut().load(scope, this);
    }
}
impl<S: FoldBySchema> DynamicFold for FoldBy<S> {
    type Output = S::Value;

    fn stop(self: Rc<Self>, scope: &BindContextScope) -> Self::Output {
        let d = &mut *(self.0).borrow_mut();
        d.load(scope, &self);
        d.bindings.clear();
        if let ScanState::Loaded(state) = take(&mut d.state) {
            d.schema.get(state)
        } else {
            panic!("invalid state.")
        }
    }
    fn as_dyn_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}

impl<S: FoldBySchema> BindSink for FoldBy<S> {
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        if self.0.borrow_mut().unload() {
            ctx.spawn(self);
        }
    }
}

impl<S: FoldBySchema> BindTask for FoldBy<S> {
    fn run(self: Rc<Self>, scope: &BindContextScope) {
        Self::next(&self, scope);
    }
}
impl<S: FoldBySchema> Drop for FoldBy<S> {
    fn drop(&mut self) {
        self.0.borrow_mut().unload();
    }
}
