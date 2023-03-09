use derive_ex::derive_ex;

use super::{
    BindSink, BindSource, ComputeContext, Computed, SinkBindings, SourceBindings, UpdateContext,
};
use crate::ObsContext;
use std::{
    cell::{RefCell, RefMut},
    rc::Rc,
};

const PARAM: usize = 0;

struct RawDependencyToken {
    data: RefCell<Data>,
    sources: RefCell<SourceBindings>,
}

#[derive_ex(Default)]
#[default(Self::new())]
pub struct DependencyToken(Rc<RawDependencyToken>);

impl DependencyToken {
    pub fn new() -> Self {
        Self(Rc::new(RawDependencyToken {
            data: RefCell::new(Data::new()),
            sources: RefCell::new(SourceBindings::new()),
        }))
    }
    pub fn is_up_to_date(&self, uc: &mut UpdateContext) -> bool {
        Helper::new(&self.0, uc).is_up_to_date().1
    }
    pub fn update<T>(&self, compute: impl FnOnce(ComputeContext) -> T, oc: &mut ObsContext) -> T {
        let mut d = self.0.data.borrow_mut();
        d.computed = Computed::UpToDate;
        let this = self.0.clone();
        d.sinks.watch(this, PARAM, oc);
        drop(d);

        let mut s = self.0.sources.borrow_mut();
        let node = Rc::downgrade(&self.0);
        s.compute(node, PARAM, compute, oc.uc)
    }
}

struct Data {
    sinks: SinkBindings,
    computed: Computed,
}
impl Data {
    fn new() -> Self {
        Self {
            sinks: SinkBindings::new(),
            computed: Computed::None,
        }
    }
}

struct Helper<'a> {
    uc: &'a mut UpdateContext,
    t: &'a RawDependencyToken,
    d: RefMut<'a, Data>,
}

impl<'a> Helper<'a> {
    fn new(t: &'a RawDependencyToken, uc: &'a mut UpdateContext) -> Self {
        Self {
            uc,
            t,
            d: t.data.borrow_mut(),
        }
    }
    fn notify(&mut self, is_modified: bool) {
        if self.d.computed.modify(is_modified) {
            self.d.sinks.notify(is_modified, self.uc);
        }
    }

    fn is_using(&mut self) -> bool {
        !self.d.sinks.is_empty()
    }
    #[allow(clippy::wrong_self_convention)]
    fn is_up_to_date(mut self) -> (Self, bool) {
        if self.d.computed == Computed::MayBeOutdated {
            self = self.flush_sources().0;
        }
        let is_up_to_date = self.d.computed == Computed::UpToDate;
        (self, is_up_to_date)
    }
    fn flush(mut self) -> (Self, bool) {
        if self.is_using() {
            self.flush_sources()
        } else {
            (self, false)
        }
    }

    fn flush_sources(mut self) -> (Self, bool) {
        drop(self.d);
        let is_modified = self.t.sources.borrow().flush(self.uc);
        self.d = self.t.data.borrow_mut();
        if !is_modified {
            self.d.computed = Computed::UpToDate;
        }
        (self, is_modified)
    }
    fn unbind_sink(&mut self, key: usize) {
        self.d.sinks.unbind(key);
    }
}
impl BindSource for RawDependencyToken {
    fn flush(self: Rc<Self>, _param: usize, uc: &mut UpdateContext) -> bool {
        Helper::new(&self, uc).flush().1
    }

    fn unbind(self: Rc<Self>, _param: usize, key: usize, uc: &mut UpdateContext) {
        Helper::new(&self, uc).unbind_sink(key)
    }
}

impl BindSink for RawDependencyToken {
    fn notify(self: Rc<Self>, _param: usize, is_modified: bool, uc: &mut UpdateContext) {
        Helper::new(&self, uc).notify(is_modified)
    }
}
