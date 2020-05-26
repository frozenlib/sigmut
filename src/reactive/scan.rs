use crate::bind::*;
use crate::reactive::*;
use derivative::Derivative;
use std::{any::Any, cell::Ref, cell::RefCell, mem::take, rc::Rc};

pub struct Scan<Loaded, Unloaded, Load, Unload, Get> {
    data: RefCell<ScanData<Loaded, Unloaded, Load, Unload, Get>>,
    sinks: BindSinks,
}
pub struct FilterScan<Loaded, Unloaded, Load, Unload, Get> {
    data: RefCell<ScanData<Loaded, Unloaded, Load, Unload, Get>>,
    sinks: BindSinks,
}

struct ScanData<Loaded, Unloaded, Load, Unload, Get> {
    load: Load,
    unload: Unload,
    get: Get,
    state: ScanState<Loaded, Unloaded>,
    bindings: Bindings,
}

#[derive(Derivative)]
#[derivative(Default(bound = ""))]
enum ScanState<Loaded, Unloaded> {
    #[derivative(Default)]
    NoData,
    Loaded(Loaded),
    Unloaded(Unloaded),
}
pub struct FilterScanResult<Loaded> {
    pub state: Loaded,
    pub is_notify: bool,
}

impl<Loaded, Unloaded> ScanState<Loaded, Unloaded> {
    fn load(
        &mut self,
        bindings: &mut Bindings,
        scope: &BindContextScope,
        sink: &Rc<impl BindSink>,
        load: impl FnOnce(Unloaded, &mut BindContext) -> Loaded,
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

    fn unload(&mut self, unload: impl FnOnce(Loaded) -> Unloaded) -> bool {
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
}
impl<T, Loaded, Unloaded, Load, Unload, Get> ScanData<Loaded, Unloaded, Load, Unload, Get>
where
    T: 'static,
    Loaded: 'static,
    Unloaded: 'static,
    Get: Fn(&Loaded) -> &T + 'static,
{
    fn get(&self) -> &T {
        if let ScanState::Loaded(state) = &self.state {
            (self.get)(state)
        } else {
            panic!("value not loaded.")
        }
    }
}

impl<T, Loaded, Unloaded, Load, Unload, Get> Scan<Loaded, Unloaded, Load, Unload, Get>
where
    T: 'static,
    Loaded: 'static,
    Unloaded: 'static,
    Load: FnMut(Unloaded, &mut BindContext) -> Loaded + 'static,
    Unload: FnMut(Loaded) -> Unloaded + 'static,
    Get: Fn(&Loaded) -> &T + 'static,
{
    pub fn new(initial_state: Unloaded, load: Load, unload: Unload, get: Get) -> Self {
        Self {
            data: RefCell::new(ScanData {
                state: ScanState::Unloaded(initial_state),
                load,
                unload,
                get,
                bindings: Bindings::new(),
            }),
            sinks: BindSinks::new(),
        }
    }
}

impl<T, Loaded, Unloaded, Load, Unload, Get> DynReBorrowSource
    for Scan<Loaded, Unloaded, Load, Unload, Get>
where
    T: 'static,
    Loaded: 'static,
    Unloaded: 'static,
    Load: FnMut(Unloaded, &mut BindContext) -> Loaded + 'static,
    Unload: FnMut(Loaded) -> Unloaded + 'static,
    Get: Fn(&Loaded) -> &T + 'static,
{
    type Item = T;

    fn dyn_borrow(
        &self,
        rc_self: &Rc<dyn DynReBorrowSource<Item = Self::Item>>,
        ctx: &mut BindContext,
    ) -> Ref<Self::Item> {
        let rc_self = Self::downcast(rc_self);
        ctx.bind(rc_self.clone());
        let mut d = self.data.borrow();
        if !d.state.is_loaded() {
            drop(d);
            {
                let d = &mut *self.data.borrow_mut();
                d.state
                    .load(&mut d.bindings, ctx.scope(), &rc_self, &mut d.load);
            }
            d = self.data.borrow();
        }
        Ref::map(d, |d| d.get())
    }
    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}

impl<T, Loaded, Unloaded, Load, Unload, Get> BindSource
    for Scan<Loaded, Unloaded, Load, Unload, Get>
where
    T: 'static,
    Loaded: 'static,
    Unloaded: 'static,
    Load: FnMut(Unloaded, &mut BindContext) -> Loaded + 'static,
    Unload: FnMut(Loaded) -> Unloaded + 'static,
    Get: Fn(&Loaded) -> &T + 'static,
{
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
    fn detach_sink(&self, idx: usize, sink: &std::rc::Weak<dyn BindSink>) {
        self.sinks.detach(idx, sink);
        if self.sinks.is_empty() {
            let d = &mut *self.data.borrow_mut();
            d.bindings.clear();
            d.state.unload(&mut d.unload);
        }
    }
}

impl<T, Loaded, Unloaded, Load, Unload, Get> BindSink for Scan<Loaded, Unloaded, Load, Unload, Get>
where
    T: 'static,
    Loaded: 'static,
    Unloaded: 'static,
    Load: FnMut(Unloaded, &mut BindContext) -> Loaded + 'static,
    Unload: FnMut(Loaded) -> Unloaded + 'static,
    Get: Fn(&Loaded) -> &T + 'static,
{
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        {
            let d = &mut *self.data.borrow_mut();
            if !d.state.unload(&mut d.unload) {
                return;
            }
        }
        self.sinks.notify(ctx);
    }
}

impl<T, Loaded, Unloaded, Load, Unload, Get> FilterScan<Loaded, Unloaded, Load, Unload, Get>
where
    T: 'static,
    Loaded: 'static,
    Unloaded: 'static,
    Load: FnMut(Unloaded, &mut BindContext) -> FilterScanResult<Loaded> + 'static,
    Unload: FnMut(Loaded) -> Unloaded + 'static,
    Get: Fn(&Loaded) -> &T + 'static,
{
    pub fn new(initial_state: Unloaded, load: Load, unload: Unload, get: Get) -> Self {
        Self {
            data: RefCell::new(ScanData {
                state: ScanState::Unloaded(initial_state),
                load,
                unload,
                get,
                bindings: Bindings::new(),
            }),
            sinks: BindSinks::new(),
        }
    }

    fn ready(self: &Rc<Self>, scope: &BindContextScope) {
        let mut is_notify = false;
        {
            let d = &mut *self.data.borrow_mut();
            if d.state.is_loaded() {
                return;
            }
            let load = &mut d.load;
            let is_notify = &mut is_notify;
            d.state
                .load(&mut d.bindings, scope, self, move |state, ctx| {
                    let r = load(state, ctx);
                    *is_notify = r.is_notify;
                    r.state
                });
        }
        if is_notify {
            self.sinks.notify_and_update();
        }
    }
}

impl<T, Loaded, Unloaded, Load, Unload, Get> DynReBorrowSource
    for FilterScan<Loaded, Unloaded, Load, Unload, Get>
where
    T: 'static,
    Loaded: 'static,
    Unloaded: 'static,
    Load: FnMut(Unloaded, &mut BindContext) -> FilterScanResult<Loaded> + 'static,
    Unload: FnMut(Loaded) -> Unloaded + 'static,
    Get: Fn(&Loaded) -> &T + 'static,
{
    type Item = T;

    fn dyn_borrow(
        &self,
        rc_self: &Rc<dyn DynReBorrowSource<Item = Self::Item>>,
        ctx: &mut BindContext,
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
}

impl<T, Loaded, Unloaded, Load, Unload, Get> BindSource
    for FilterScan<Loaded, Unloaded, Load, Unload, Get>
where
    T: 'static,
    Loaded: 'static,
    Unloaded: 'static,
    Load: FnMut(Unloaded, &mut BindContext) -> FilterScanResult<Loaded> + 'static,
    Unload: FnMut(Loaded) -> Unloaded + 'static,
    Get: Fn(&Loaded) -> &T + 'static,
{
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
    fn detach_sink(&self, idx: usize, sink: &std::rc::Weak<dyn BindSink>) {
        self.sinks.detach(idx, sink);
        if self.sinks.is_empty() {
            let d = &mut *self.data.borrow_mut();
            d.bindings.clear();
            d.state.unload(&mut d.unload);
        }
    }
}

impl<T, Loaded, Unloaded, Load, Unload, Get> BindSink
    for FilterScan<Loaded, Unloaded, Load, Unload, Get>
where
    T: 'static,
    Loaded: 'static,
    Unloaded: 'static,
    Load: FnMut(Unloaded, &mut BindContext) -> FilterScanResult<Loaded> + 'static,
    Unload: FnMut(Loaded) -> Unloaded + 'static,
    Get: Fn(&Loaded) -> &T + 'static,
{
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        let d = &mut *self.data.borrow_mut();
        if d.state.unload(&mut d.unload) {
            if !self.sinks.is_empty() {
                ctx.spawn(Rc::downgrade(&self));
            }
        }
    }
}
impl<T, Loaded, Unloaded, Load, Unload, Get> Task
    for FilterScan<Loaded, Unloaded, Load, Unload, Get>
where
    T: 'static,
    Loaded: 'static,
    Unloaded: 'static,
    Load: FnMut(Unloaded, &mut BindContext) -> FilterScanResult<Loaded> + 'static,
    Unload: FnMut(Loaded) -> Unloaded + 'static,
    Get: Fn(&Loaded) -> &T + 'static,
{
    fn run(self: Rc<Self>, scope: &BindContextScope) {
        self.ready(scope);
    }
}

pub struct FoldBy<St, Loaded, Load, Unload, Get>(
    RefCell<ScanData<(St, Loaded), St, Load, Unload, Get>>,
)
where
    Load: FnMut(St, &mut BindContext) -> (St, Loaded) + 'static,
    Unload: FnMut((St, Loaded)) -> St + 'static;

impl<T, St, Loaded, Load, Unload, Get> FoldBy<St, Loaded, Load, Unload, Get>
where
    St: 'static,
    Loaded: 'static,
    Load: FnMut(St, &mut BindContext) -> (St, Loaded) + 'static,
    Unload: FnMut((St, Loaded)) -> St + 'static,
    Get: FnMut(St) -> T + 'static,
{
    pub fn new(initial_state: St, load: Load, unload: Unload, get: Get) -> Rc<Self> {
        let this = Rc::new(FoldBy(RefCell::new(ScanData {
            state: ScanState::Unloaded(initial_state),
            load,
            unload,
            get,
            bindings: Bindings::new(),
        })));
        BindContextScope::with(|scope| Self::next(&this, scope));
        this
    }
    fn next(this: &Rc<Self>, scope: &BindContextScope) {
        let d = &mut *this.0.borrow_mut();
        d.state.load(&mut d.bindings, scope, this, &mut d.load);
    }
}
impl<T, St, Loaded, Load, Unload, Get> DynFold for FoldBy<St, Loaded, Load, Unload, Get>
where
    St: 'static,
    Loaded: 'static,
    Load: FnMut(St, &mut BindContext) -> (St, Loaded) + 'static,
    Unload: FnMut((St, Loaded)) -> St + 'static,
    Get: FnMut(St) -> T + 'static,
{
    type Output = T;

    fn stop(&self) -> Self::Output {
        let d = &mut *(self.0).borrow_mut();
        d.state.unload(&mut d.unload);
        let s = match take(&mut d.state) {
            ScanState::Loaded((s, _loaded)) => s,
            ScanState::Unloaded(s) => s,
            ScanState::NoData => panic!("invalid state."),
        };
        (d.get)(s)
    }
    fn as_dyn_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}

impl<T, St, Loaded, Load, Unload, Get> BindSink for FoldBy<St, Loaded, Load, Unload, Get>
where
    St: 'static,
    Loaded: 'static,
    Load: FnMut(St, &mut BindContext) -> (St, Loaded) + 'static,
    Unload: FnMut((St, Loaded)) -> St + 'static,
    Get: FnMut(St) -> T + 'static,
{
    fn notify(self: Rc<Self>, ctx: &NotifyContext) {
        let d = &mut *self.0.borrow_mut();
        if d.state.unload(&mut d.unload) {
            ctx.spawn(Rc::downgrade(&self));
        }
    }
}

impl<T, St, Loaded, Load, Unload, Get> Task for FoldBy<St, Loaded, Load, Unload, Get>
where
    St: 'static,
    Loaded: 'static,
    Load: FnMut(St, &mut BindContext) -> (St, Loaded) + 'static,
    Unload: FnMut((St, Loaded)) -> St + 'static,
    Get: FnMut(St) -> T + 'static,
{
    fn run(self: Rc<Self>, scope: &BindContextScope) {
        Self::next(&self, scope);
    }
}
impl<St, Loaded, Load, Unload, Get> Drop for FoldBy<St, Loaded, Load, Unload, Get>
where
    Load: FnMut(St, &mut BindContext) -> (St, Loaded) + 'static,
    Unload: FnMut((St, Loaded)) -> St + 'static,
{
    fn drop(&mut self) {
        let d = &mut *self.0.borrow_mut();
        d.state.unload(&mut d.unload);
    }
}
