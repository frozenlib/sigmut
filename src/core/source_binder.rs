use std::rc::Weak;

use crate::SignalContext;

use super::{BindSink, Dirty, DirtyLevel, ReactionContext, Slot, SourceBindings};

pub struct SourceBinder {
    sources: SourceBindings,
    dirty: Dirty,
    sink: Weak<dyn BindSink>,
    slot: Slot,
}
impl SourceBinder {
    pub fn new(sink: &Weak<impl BindSink>, slot: Slot) -> Self {
        Self {
            sources: SourceBindings::new(),
            dirty: Dirty::Dirty,
            sink: sink.clone(),
            slot,
        }
    }
    pub fn is_clean(&self) -> bool {
        self.dirty.is_clean()
    }

    pub fn check(&mut self, rc: &mut ReactionContext) -> bool {
        self.sources.check_with(&mut self.dirty, rc)
    }
    pub fn update<T>(
        &mut self,
        f: impl FnOnce(&mut SignalContext) -> T,
        rc: &mut ReactionContext,
    ) -> T {
        self.dirty = Dirty::Clean;
        self.sources
            .update(self.sink.clone(), self.slot, true, f, rc)
    }
    pub fn clear(&mut self, rc: &mut ReactionContext) {
        self.sources.clear(rc);
        self.dirty = Dirty::Dirty;
    }
    /// Set the state to dirty and return true if the dependants need to be notified.
    pub fn on_notify(&mut self, slot: Slot, level: DirtyLevel) -> bool {
        let mut needs_notify = false;
        if slot == self.slot {
            needs_notify = self.dirty.needs_notify();
            self.dirty.apply_notify(level);
        }
        needs_notify
    }
}
