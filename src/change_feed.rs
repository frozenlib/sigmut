//! Change feed building blocks for reactive collection types.
//!
//! A change feed model stores its latest value together with model-specific changes. A reader's
//! first read is [`ChangeFeedDelta::Initial`]; later reads provide the recorded changes through
//! [`ChangeFeedDelta::Changes`]. Unread changes remain available until the reader advances or is
//! dropped. Use [`ChangeFeedState`] for mutable state and [`ChangeFeedSignal::from_scan`] for
//! derived values.
//!
//! Mutating [`ChangeFeedRefMut::current_mut`] does not record a change by itself. Every observable
//! mutation must be paired with [`ChangeFeedRefMut::record`] in the same mutable borrow.
//!
//! # Examples
//!
//! ```
//! use sigmut::{
//!     core::Runtime,
//!     change_feed::{ChangeFeedDelta, ChangeFeedRefMut, ChangeFeedModel, ChangeFeedState},
//! };
//!
//! struct Counter(i32);
//!
//! impl ChangeFeedModel for Counter {
//!     type Change = i32;
//! }
//!
//! fn set(edit: &mut ChangeFeedRefMut<'_, Counter>, value: i32) {
//!     let old = edit.current().0;
//!     edit.current_mut().0 = value;
//!     edit.record(old);
//! }
//!
//! let mut runtime = Runtime::new();
//! let state = ChangeFeedState::new(Counter(0));
//! let mut reader = state.reader();
//!
//! assert!(matches!(
//!     reader.read(&mut runtime.sc()).delta(),
//!     ChangeFeedDelta::Initial,
//! ));
//!
//! set(&mut state.borrow_mut(runtime.ac()), 1);
//! match reader.read(&mut runtime.sc()).delta() {
//!     ChangeFeedDelta::Initial => unreachable!(),
//!     ChangeFeedDelta::Changes(changes) => {
//!         assert_eq!(changes.copied().collect::<Vec<_>>(), [0]);
//!     }
//! }
//! ```

use std::{
    any::Any,
    cell::{Cell, Ref, RefCell, RefMut},
    mem,
    rc::{Rc, Weak},
};

use derive_ex::Ex;

use crate::{
    ActionContext, SignalContext,
    core::{
        BindKey, BindSink, BindSource, DirtyLevel, NotifyContext, ReactionContext, SinkBindings,
        Slot, SourceBinder, schedule_notify,
    },
    utils::{Changes, RefCountOps},
};

#[cfg(test)]
mod tests;

/// Defines the latest value and change type exposed through a change feed.
pub trait ChangeFeedModel: 'static {
    /// A model-specific recorded change.
    type Change: 'static;

    /// Releases resources retained by a change that is no longer readable.
    ///
    /// The default implementation drops `change`. Models that keep referenced values in the
    /// current model can remove those values here.
    fn release_change(&mut self, change: Self::Change) {
        drop(change);
    }
}

#[derive(Clone, Copy)]
struct Cursor(usize);

struct History<M: ChangeFeedModel> {
    current: M,
    changes: Changes<M::Change>,
}

impl<M: ChangeFeedModel> History<M> {
    fn new(current: M) -> Self {
        Self {
            current,
            changes: Changes::new(),
        }
    }

    fn end_cursor(&self) -> Cursor {
        Cursor(self.changes.end_age())
    }
}

struct ChangeFeedStorageCell<M: ChangeFeedModel> {
    history: RefCell<History<M>>,
    reader_ops: RefCell<RefCountOps>,
}

#[derive(Ex)]
#[derive_ex(Clone(bound()))]
pub(crate) struct ChangeFeedStorage<M: ChangeFeedModel>(Rc<ChangeFeedStorageCell<M>>);

impl<M: ChangeFeedModel> ChangeFeedStorage<M> {
    pub(crate) fn new(current: M) -> Self {
        Self(Rc::new(ChangeFeedStorageCell {
            history: RefCell::new(History::new(current)),
            reader_ops: RefCell::new(RefCountOps::new()),
        }))
    }

    fn borrow_since(&self, since: Option<Cursor>) -> ChangeFeedRef<'_, M> {
        self.compact();
        ChangeFeedRef {
            storage: self,
            history: Some(self.0.history.borrow()),
            since,
        }
    }

    pub(crate) fn borrow_current(&self) -> ChangeFeedRef<'_, M> {
        self.compact();
        let history = self.0.history.borrow();
        let since = Some(history.end_cursor());
        ChangeFeedRef {
            storage: self,
            history: Some(history),
            since,
        }
    }

    pub(crate) fn current_ref(&self) -> Ref<'_, M> {
        self.compact();
        Ref::map(self.0.history.borrow(), |history| &history.current)
    }

    pub(crate) fn begin_edit(&self) -> ChangeFeedRefMut<'_, M> {
        self.compact();
        let history = self.0.history.borrow_mut();
        let start = history.end_cursor();
        ChangeFeedRefMut {
            storage: self,
            history: Some(history),
            start,
            dirty: false,
            finish: EditFinish::None,
        }
    }

    pub(crate) fn reader(&self) -> ChangeFeedCursorReader<M> {
        ChangeFeedCursorReader {
            storage: self.clone(),
            cursor: None,
        }
    }

    fn retain_cursor(&self, cursor: Cursor) {
        self.0.reader_ops.borrow_mut().increment_at(cursor.0);
        self.compact();
    }

    fn advance_cursor(&self, old: Option<Cursor>) -> Cursor {
        let end = self.0.history.borrow().end_cursor();
        {
            let mut reader_ops = self.0.reader_ops.borrow_mut();
            reader_ops.decrement(old.map(|cursor| cursor.0));
            reader_ops.increment();
        }
        self.compact();
        end
    }

    fn release_cursor(&self, cursor: Cursor) {
        self.0.reader_ops.borrow_mut().decrement(Some(cursor.0));
        self.compact();
    }

    fn compact(&self) {
        let Ok(mut history) = self.0.history.try_borrow_mut() else {
            return;
        };
        let History { current, changes } = &mut *history;
        self.0.reader_ops.borrow_mut().apply(changes);
        changes.clean(|change| current.release_change(change));
    }
}

/// Distinguishes an initial read from changes after a reader cursor.
#[must_use]
pub enum ChangeFeedDelta<I> {
    /// No earlier reader cursor exists.
    Initial,
    /// Changes recorded after the earlier reader cursor.
    Changes(I),
}

/// A borrowed latest value and the changes after a reader cursor.
///
/// This guard cannot access an earlier value. The latest value and referenced changes remain
/// stable until the guard is dropped.
pub struct ChangeFeedRef<'a, M: ChangeFeedModel> {
    storage: &'a ChangeFeedStorage<M>,
    history: Option<Ref<'a, History<M>>>,
    since: Option<Cursor>,
}

impl<M: ChangeFeedModel> ChangeFeedRef<'_, M> {
    fn history(&self) -> &History<M> {
        self.history.as_deref().unwrap()
    }

    /// Returns the current model value.
    pub fn current(&self) -> &M {
        &self.history().current
    }

    /// Returns whether this is an initial read or changes after an earlier read.
    pub fn delta(&self) -> ChangeFeedDelta<impl Iterator<Item = &M::Change> + '_> {
        match self.since {
            None => ChangeFeedDelta::Initial,
            Some(cursor) => ChangeFeedDelta::Changes(self.history().changes.items(cursor.0)),
        }
    }
}

impl<M: ChangeFeedModel> Drop for ChangeFeedRef<'_, M> {
    fn drop(&mut self) {
        drop(self.history.take());
        self.storage.compact();
    }
}

enum EditFinish<'a> {
    None,
    Track(&'a Cell<bool>),
    Notify {
        sinks: &'a RefCell<SinkBindings>,
        nc: &'a mut NotifyContext,
    },
    Schedule(Weak<dyn BindSink>),
}

/// A mutable borrowed reference that records model-specific changes.
///
/// Every observable mutation through [`current_mut`](Self::current_mut) must be paired with a call
/// to [`record`](Self::record). This guard deliberately does not implement
/// [`DerefMut`](std::ops::DerefMut). A guard that records no changes does not notify dependants.
pub struct ChangeFeedRefMut<'a, M: ChangeFeedModel> {
    storage: &'a ChangeFeedStorage<M>,
    history: Option<RefMut<'a, History<M>>>,
    start: Cursor,
    dirty: bool,
    finish: EditFinish<'a>,
}

impl<'a, M: ChangeFeedModel> ChangeFeedRefMut<'a, M> {
    fn history(&self) -> &History<M> {
        self.history.as_deref().unwrap()
    }

    fn history_mut(&mut self) -> &mut History<M> {
        self.history.as_deref_mut().unwrap()
    }

    fn with_finish(mut self, finish: EditFinish<'a>) -> Self {
        self.finish = finish;
        self
    }

    pub(crate) fn is_dirty(&self) -> bool {
        debug_assert_eq!(self.dirty, self.start.0 != self.history().changes.end_age());
        self.dirty
    }

    /// Returns the current model value.
    pub fn current(&self) -> &M {
        &self.history().current
    }

    /// Returns the current model value mutably.
    ///
    /// Mutating the value does not record a change automatically. Call [`record`](Self::record)
    /// for every observable mutation before this edit is dropped.
    pub fn current_mut(&mut self) -> &mut M {
        &mut self.history_mut().current
    }

    /// Appends a model-specific change to the feed.
    pub fn record(&mut self, change: M::Change) {
        self.history_mut().changes.push(change);
        self.dirty = true;
    }
}

impl<M: ChangeFeedModel> Drop for ChangeFeedRefMut<'_, M> {
    fn drop(&mut self) {
        let dirty = self.is_dirty();
        drop(self.history.take());
        self.storage.compact();
        if !dirty {
            return;
        }
        match mem::replace(&mut self.finish, EditFinish::None) {
            EditFinish::None => {}
            EditFinish::Track(dirty) => dirty.set(true),
            EditFinish::Notify { sinks, nc } => {
                sinks.borrow_mut().notify(DirtyLevel::Dirty, nc);
            }
            EditFinish::Schedule(node) => schedule_notify(node, Slot(0)),
        }
    }
}

pub(crate) struct ChangeFeedCursorReader<M: ChangeFeedModel> {
    storage: ChangeFeedStorage<M>,
    cursor: Option<Cursor>,
}

impl<M: ChangeFeedModel> ChangeFeedCursorReader<M> {
    pub(crate) fn read(&mut self) -> ChangeFeedRef<'_, M> {
        let cursor = self.cursor;
        let value = self.storage.borrow_since(cursor);
        self.cursor = Some(self.storage.advance_cursor(cursor));
        value
    }

    pub(crate) fn peek(&self) -> ChangeFeedRef<'_, M> {
        self.storage.borrow_since(self.cursor)
    }
}

impl<M: ChangeFeedModel> Clone for ChangeFeedCursorReader<M> {
    fn clone(&self) -> Self {
        let storage = self.storage.clone();
        if let Some(cursor) = self.cursor {
            storage.retain_cursor(cursor);
        }
        Self {
            storage,
            cursor: self.cursor,
        }
    }
}

impl<M: ChangeFeedModel> Drop for ChangeFeedCursorReader<M> {
    fn drop(&mut self) {
        if let Some(cursor) = self.cursor {
            self.storage.release_cursor(cursor);
        }
    }
}

trait ChangeFeedNode<M: ChangeFeedModel>: Any {
    fn to_any(self: Rc<Self>) -> Rc<dyn Any>;
    fn watch(&self, rc_self: Rc<dyn Any>, sc: &mut SignalContext<'_, '_>);
    fn storage(&self) -> &ChangeFeedStorage<M>;
}

/// A reactive signal that exposes its latest value and model-specific changes.
#[derive(Ex)]
#[derive_ex(Clone(bound()))]
pub struct ChangeFeedSignal<M: ChangeFeedModel>(Rc<dyn ChangeFeedNode<M>>);

impl<M: ChangeFeedModel> ChangeFeedSignal<M> {
    /// Creates a derived change feed signal.
    ///
    /// The update function receives ownership of an edit. Recorded changes determine whether the
    /// signal became dirty.
    pub fn from_scan(
        initial: M,
        f: impl FnMut(ChangeFeedRefMut<'_, M>, &mut SignalContext<'_, '_>) + 'static,
    ) -> Self {
        Self(ScanNode::new(initial, f))
    }

    /// Borrows the current model value and registers a dependency.
    ///
    /// The returned reference represents the current position and therefore contains no pending
    /// changes.
    pub fn borrow<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> ChangeFeedRef<'a, M> {
        self.watch(sc);
        self.0.storage().borrow_current()
    }

    /// Creates a reader starting before its first read.
    pub fn reader(&self) -> ChangeFeedReader<M> {
        ChangeFeedReader {
            source: self.clone(),
            cursor: self.0.storage().reader(),
        }
    }

    fn watch(&self, sc: &mut SignalContext<'_, '_>) {
        self.0.watch(self.0.clone().to_any(), sc);
    }
}

/// A reader that independently advances through a change feed.
pub struct ChangeFeedReader<M: ChangeFeedModel> {
    source: ChangeFeedSignal<M>,
    cursor: ChangeFeedCursorReader<M>,
}

impl<M: ChangeFeedModel> ChangeFeedReader<M> {
    /// Borrows the latest value and advances this reader to the current end.
    pub fn read<'a, 'r: 'a>(&'a mut self, sc: &mut SignalContext<'r, '_>) -> ChangeFeedRef<'a, M> {
        self.source.watch(sc);
        self.cursor.read()
    }

    /// Borrows the latest value without advancing this reader.
    pub fn peek<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> ChangeFeedRef<'a, M> {
        self.source.watch(sc);
        self.cursor.peek()
    }
}

impl<M: ChangeFeedModel> Clone for ChangeFeedReader<M> {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            cursor: self.cursor.clone(),
        }
    }
}

/// Mutable reactive state that exposes its latest value and model-specific changes.
#[derive(Ex)]
#[derive_ex(Clone(bound()))]
pub struct ChangeFeedState<M: ChangeFeedModel>(Rc<StateNode<M>>);

impl<M: ChangeFeedModel> ChangeFeedState<M> {
    /// Creates change feed state with an initial model value.
    pub fn new(initial: M) -> Self {
        Self(Rc::new(StateNode {
            storage: ChangeFeedStorage::new(initial),
            sinks: RefCell::new(SinkBindings::new()),
        }))
    }

    /// Borrows the current model value and registers a dependency.
    pub fn borrow<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> ChangeFeedRef<'a, M> {
        self.0.bind(sc);
        self.0.storage.borrow_current()
    }

    /// Borrows the current model value without registering a dependency.
    pub fn borrow_untracked(&self) -> ChangeFeedRef<'_, M> {
        self.0.storage.borrow_current()
    }

    /// Mutably borrows the model for a change-recording edit.
    pub fn borrow_mut<'a>(&'a self, ac: &'a mut ActionContext) -> ChangeFeedRefMut<'a, M> {
        self.0.storage.begin_edit().with_finish(EditFinish::Notify {
            sinks: &self.0.sinks,
            nc: ac.nc(),
        })
    }

    /// Mutably borrows the model without tying the returned guard to the action context.
    ///
    /// A recorded change schedules notification after the edit is dropped.
    pub fn borrow_mut_loose(&self, _ac: &mut ActionContext) -> ChangeFeedRefMut<'_, M> {
        let node: Rc<dyn BindSink> = self.0.clone();
        self.0
            .storage
            .begin_edit()
            .with_finish(EditFinish::Schedule(Rc::downgrade(&node)))
    }

    /// Returns a signal backed by this state.
    pub fn to_signal(&self) -> ChangeFeedSignal<M> {
        ChangeFeedSignal(self.0.clone())
    }

    /// Creates a reader backed by this state.
    pub fn reader(&self) -> ChangeFeedReader<M> {
        self.to_signal().reader()
    }

    pub(crate) fn current_ref_untracked(&self) -> Ref<'_, M> {
        self.0.storage.current_ref()
    }
}

struct StateNode<M: ChangeFeedModel> {
    storage: ChangeFeedStorage<M>,
    sinks: RefCell<SinkBindings>,
}

impl<M: ChangeFeedModel> StateNode<M> {
    fn bind(self: &Rc<Self>, sc: &mut SignalContext<'_, '_>) {
        self.sinks.borrow_mut().bind(self.clone(), Slot(0), sc);
    }
}

impl<M: ChangeFeedModel> ChangeFeedNode<M> for StateNode<M> {
    fn to_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }

    fn watch(&self, rc_self: Rc<dyn Any>, sc: &mut SignalContext<'_, '_>) {
        rc_self.downcast::<Self>().unwrap().bind(sc);
    }

    fn storage(&self) -> &ChangeFeedStorage<M> {
        &self.storage
    }
}

impl<M: ChangeFeedModel> BindSource for StateNode<M> {
    fn check(self: Rc<Self>, _slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) -> bool {
        self.sinks.borrow().is_dirty(key, rc)
    }

    fn unbind(self: Rc<Self>, _slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) {
        self.sinks.borrow_mut().unbind(key, rc);
    }

    fn rebind(self: Rc<Self>, slot: Slot, key: BindKey, sc: &mut SignalContext<'_, '_>) {
        self.sinks.borrow_mut().rebind(self.clone(), slot, key, sc);
    }
}

impl<M: ChangeFeedModel> BindSink for StateNode<M> {
    fn notify(self: Rc<Self>, _slot: Slot, level: DirtyLevel, nc: &mut NotifyContext) {
        self.sinks.borrow_mut().notify(level, nc);
    }
}

struct ScanNode<M: ChangeFeedModel, F> {
    storage: ChangeFeedStorage<M>,
    data: RefCell<ScanData<F>>,
    sinks: RefCell<SinkBindings>,
}

impl<M, F> ScanNode<M, F>
where
    M: ChangeFeedModel,
    F: FnMut(ChangeFeedRefMut<'_, M>, &mut SignalContext<'_, '_>) + 'static,
{
    fn new(initial: M, f: F) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            storage: ChangeFeedStorage::new(initial),
            data: RefCell::new(ScanData {
                source_binder: SourceBinder::new(this, Slot(0)),
                f,
            }),
            sinks: RefCell::new(SinkBindings::new()),
        })
    }

    fn update(self: &Rc<Self>, rc: &mut ReactionContext<'_, '_>) {
        if rc.borrow(&self.data).source_binder.is_clean() {
            return;
        }
        let data = &mut *self.data.borrow_mut();
        let dirty = Cell::new(false);
        if data.source_binder.check(rc) {
            let edit = self
                .storage
                .begin_edit()
                .with_finish(EditFinish::Track(&dirty));
            data.source_binder.update(|sc| (data.f)(edit, sc), rc);
        }
        self.sinks.borrow_mut().update(dirty.get(), rc);
    }

    fn watch(self: &Rc<Self>, sc: &mut SignalContext<'_, '_>) {
        self.update(sc.rc());
        self.sinks.borrow_mut().bind(self.clone(), Slot(0), sc);
    }
}

impl<M, F> ChangeFeedNode<M> for ScanNode<M, F>
where
    M: ChangeFeedModel,
    F: FnMut(ChangeFeedRefMut<'_, M>, &mut SignalContext<'_, '_>) + 'static,
{
    fn to_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }

    fn watch(&self, rc_self: Rc<dyn Any>, sc: &mut SignalContext<'_, '_>) {
        rc_self.downcast::<Self>().unwrap().watch(sc);
    }

    fn storage(&self) -> &ChangeFeedStorage<M> {
        &self.storage
    }
}

impl<M, F> BindSource for ScanNode<M, F>
where
    M: ChangeFeedModel,
    F: FnMut(ChangeFeedRefMut<'_, M>, &mut SignalContext<'_, '_>) + 'static,
{
    fn check(self: Rc<Self>, _slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) -> bool {
        self.update(rc);
        self.sinks.borrow().is_dirty(key, rc)
    }

    fn unbind(self: Rc<Self>, _slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) {
        self.sinks.borrow_mut().unbind(key, rc);
    }

    fn rebind(self: Rc<Self>, slot: Slot, key: BindKey, sc: &mut SignalContext<'_, '_>) {
        self.sinks.borrow_mut().rebind(self.clone(), slot, key, sc);
    }
}

impl<M, F> BindSink for ScanNode<M, F>
where
    M: ChangeFeedModel,
    F: FnMut(ChangeFeedRefMut<'_, M>, &mut SignalContext<'_, '_>) + 'static,
{
    fn notify(self: Rc<Self>, slot: Slot, level: DirtyLevel, nc: &mut NotifyContext) {
        if self.data.borrow_mut().source_binder.on_notify(slot, level) {
            self.sinks.borrow_mut().notify(DirtyLevel::MaybeDirty, nc);
        }
    }
}

struct ScanData<F> {
    source_binder: SourceBinder,
    f: F,
}
