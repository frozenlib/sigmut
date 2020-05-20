use crate::bind::*;
use crate::reactive::*;
use std::mem::replace;
use std::{any::Any, cell::Ref, cell::RefCell, rc::Rc};

pub struct Scan<Loaded, Unloaded, Load, Unload, Get> {
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

pub enum ScanState<Loaded, Unloaded> {
    NoData,
    Loaded(Loaded),
    Unloaded(Unloaded),
}
impl<Loaded, Unloaded> ScanState<Loaded, Unloaded> {
    fn load(&mut self, load: impl FnMut(Unloaded) -> Loaded) -> bool {
        let mut load = load;
        if let ScanState::Unloaded(_) = self {
            if let Self::Unloaded(value) = replace(self, Self::NoData) {
                *self = Self::Loaded(load(value));
                return true;
            }
        }
        false
    }
    fn unload(&mut self, unload: impl FnMut(Loaded) -> Unloaded) -> bool {
        let mut unload = unload;
        if let ScanState::Loaded(_) = self {
            if let Self::Loaded(value) = replace(self, Self::NoData) {
                *self = Self::Unloaded(unload(value));
                return true;
            }
        }
        false
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
    pub fn new(state: ScanState<Loaded, Unloaded>, load: Load, unload: Unload, get: Get) -> Self {
        Self {
            data: RefCell::new(ScanData {
                state,
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
        let mut s = self.data.borrow();
        if let ScanState::Unloaded(_) = s.state {
            drop(s);
            let mut b = self.data.borrow_mut();
            let d = &mut *b;
            let load = &mut d.load;
            d.state.load(|state| load(state, ctx));
            drop(b);
            s = self.data.borrow();
        }
        return Ref::map(s, |s| {
            if let ScanState::Loaded(loaded) = &s.state {
                (s.get)(loaded)
            } else {
                unreachable!()
            }
        });
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
        self.sinks().detach(idx, sink);
        if self.sinks().is_empty() {
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
        let mut b = self.data.borrow_mut();
        let d = &mut *b;
        if d.state.unload(&mut d.unload) {
            drop(b);
            self.sinks.notify(ctx);
        }
    }
}
