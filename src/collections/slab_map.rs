//! Reactive slab maps with live-key dependencies and incremental change readers.

use std::{
    any::Any,
    cell::{Ref, RefCell},
    ops::Index,
    rc::Rc,
};

use derive_ex::derive_ex;
use slabmap::SlabMap;

use crate::{
    ActionContext, SignalContext,
    building_blocks::change_feed::{
        ChangeFeedCursorReader, ChangeFeedDelta, ChangeFeedModel, ChangeFeedReader, ChangeFeedRef,
        ChangeFeedRefMut, ChangeFeedState, ChangeFeedStorage,
    },
    core::{
        BindKey, BindSink, BindSource, DirtyLevel, NotifyContext, ReactionContext, SinkBindings,
        Slot, SourceBinder,
    },
};

const SLOT_ITEMS: Slot = Slot(usize::MAX);

fn key_to_slot(key: usize) -> Slot {
    assert!(key != usize::MAX);
    Slot(key)
}

fn slot_to_key(slot: Slot) -> Option<usize> {
    if slot == SLOT_ITEMS {
        None
    } else {
        Some(slot.0)
    }
}

/// A reactive read-only slab map.
pub struct SignalSlabMap<T>(Rc<dyn DynSignalSlabMap<T>>);

impl<T: 'static> SignalSlabMap<T> {
    /// Creates a derived slab map.
    pub fn from_scan(
        f: impl FnMut(&mut SlabMapRefMut<'_, T>, &mut SignalContext<'_, '_>) + 'static,
    ) -> Self {
        Self(Scan::new(f))
    }

    /// Borrows an item and registers a dependency on its key.
    ///
    /// # Panics
    ///
    /// Panics if `key` is not live.
    pub fn item<'a, 'r: 'a>(&'a self, key: usize, sc: &mut SignalContext<'r, '_>) -> Ref<'a, T> {
        self.0.item(self.0.clone().to_any(), key, sc)
    }

    /// Borrows all current items and registers a collection-wide dependency.
    pub fn borrow<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> SlabMapRef<'a, T> {
        self.0.borrow(self.0.clone().to_any(), sc)
    }

    /// Creates an independently advancing change reader.
    pub fn reader(&self) -> SignalSlabMapReader<T> {
        self.0.clone().reader()
    }
}

trait DynSignalSlabMap<T> {
    fn to_any(self: Rc<Self>) -> Rc<dyn Any>;
    fn item(&self, rc_self: Rc<dyn Any>, key: usize, sc: &mut SignalContext<'_, '_>) -> Ref<'_, T>;
    fn borrow<'a, 'r: 'a>(
        &'a self,
        rc_self: Rc<dyn Any>,
        sc: &mut SignalContext<'r, '_>,
    ) -> SlabMapRef<'a, T>;
    fn watch_all(&self, rc_self: Rc<dyn Any>, sc: &mut SignalContext<'_, '_>);
    fn reader(self: Rc<Self>) -> SignalSlabMapReader<T>;
}

/// An independently advancing reader for a [`SignalSlabMap`].
#[derive_ex(Clone(bound()))]
pub struct SignalSlabMapReader<T: 'static>(RawSignalSlabMapReader<T>);

#[derive_ex(Clone(bound()))]
enum RawSignalSlabMapReader<T: 'static> {
    State(ChangeFeedReader<SlabMapModel<T>>),
    Scan {
        source: Rc<dyn DynSignalSlabMap<T>>,
        cursor: ChangeFeedCursorReader<SlabMapModel<T>>,
    },
}

impl<T: 'static> SignalSlabMapReader<T> {
    fn from_state(reader: ChangeFeedReader<SlabMapModel<T>>) -> Self {
        Self(RawSignalSlabMapReader::State(reader))
    }

    fn from_scan(
        source: Rc<dyn DynSignalSlabMap<T>>,
        cursor: ChangeFeedCursorReader<SlabMapModel<T>>,
    ) -> Self {
        Self(RawSignalSlabMapReader::Scan { source, cursor })
    }

    /// Reads the current items and advances this reader's cursor.
    pub fn read<'a, 'r: 'a>(&'a mut self, sc: &mut SignalContext<'r, '_>) -> SlabMapRef<'a, T> {
        match &mut self.0 {
            RawSignalSlabMapReader::State(reader) => SlabMapRef::new(reader.read(sc)),
            RawSignalSlabMapReader::Scan { source, cursor } => {
                source.watch_all(source.clone().to_any(), sc);
                SlabMapRef::new(cursor.read())
            }
        }
    }

    /// Reads without advancing this reader's cursor.
    pub fn peek<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> SlabMapRef<'a, T> {
        match &self.0 {
            RawSignalSlabMapReader::State(reader) => SlabMapRef::new(reader.peek(sc)),
            RawSignalSlabMapReader::Scan { source, cursor } => {
                source.watch_all(source.clone().to_any(), sc);
                SlabMapRef::new(cursor.peek())
            }
        }
    }
}

/// A borrowed reference to the current slab map and its reader delta.
pub struct SlabMapRef<'a, T: 'static> {
    value: ChangeFeedRef<'a, SlabMapModel<T>>,
}

impl<'a, T: 'static> SlabMapRef<'a, T> {
    fn new(value: ChangeFeedRef<'a, SlabMapModel<T>>) -> Self {
        Self { value }
    }

    /// Returns whether the map contains no items.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the number of items.
    pub fn len(&self) -> usize {
        self.value.current().len
    }

    /// Returns an item for a live key.
    ///
    /// # Panics
    ///
    /// Panics if `key` is not live.
    pub fn item(&self, key: usize) -> &T {
        self.value.current().item(key)
    }

    /// Iterates over live keys and items.
    pub fn iter(&self) -> Iter<'_, T> {
        self.value.current().iter()
    }

    /// Returns the delta needed to reproduce the current items from the reader's baseline.
    ///
    /// An initial read returns an [`Insert`](SlabMapChange::Insert) for every current item.
    pub fn delta(&self) -> impl Iterator<Item = SlabMapChange<'_, T>> {
        use iter_n::iter2::*;
        match self.value.delta() {
            ChangeFeedDelta::Initial => self
                .iter()
                .map(|(key, new_value)| SlabMapChange::Insert { key, new_value })
                .into_iter0(),
            ChangeFeedDelta::Incremental(changes) => changes
                .map(|change| {
                    let value = &self.value.current().items[change.key].value;
                    match change.action {
                        ChangeAction::Insert => SlabMapChange::Insert {
                            key: change.key,
                            new_value: value,
                        },
                        ChangeAction::Remove => SlabMapChange::Remove {
                            key: change.key,
                            old_value: value,
                        },
                    }
                })
                .into_iter1(),
        }
    }
}

impl<T: 'static> Index<usize> for SlabMapRef<'_, T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.item(index)
    }
}

impl<'a, T: 'static> IntoIterator for &'a SlabMapRef<'a, T> {
    type Item = (usize, &'a T);
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// A mutable borrowed reference that records slab map changes.
///
/// This guard exposes only operations that record their changes and deliberately does not
/// implement [`DerefMut`](std::ops::DerefMut).
pub struct SlabMapRefMut<'a, T: 'static> {
    edit: ChangeFeedRefMut<'a, SlabMapModel<T>>,
}

impl<T: 'static> SlabMapRefMut<'_, T> {
    /// Returns whether the map contains no items.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the number of items.
    pub fn len(&self) -> usize {
        self.edit.current().len
    }

    /// Returns an item for a live key.
    ///
    /// # Panics
    ///
    /// Panics if `key` is not live.
    pub fn item(&self, key: usize) -> &T {
        self.edit.current().item(key)
    }

    /// Inserts an item and returns its key.
    pub fn insert(&mut self, value: T) -> usize {
        let (key, change) = self.edit.current_mut().insert(value);
        self.edit.record(change);
        key
    }

    /// Removes an item.
    ///
    /// # Panics
    ///
    /// Panics if `key` is not live.
    pub fn remove(&mut self, key: usize) {
        let change = self.edit.current_mut().remove(key);
        self.edit.record(change);
    }

    /// Iterates over live keys and items.
    pub fn iter(&self) -> Iter<'_, T> {
        self.edit.current().iter()
    }
}

impl<T: 'static> Index<usize> for SlabMapRefMut<'_, T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.item(index)
    }
}

impl<'a, T: 'static> IntoIterator for &'a SlabMapRefMut<'_, T> {
    type Item = (usize, &'a T);
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

struct SlabMapModel<T> {
    items: SlabMap<Item<T>>,
    len: usize,
}

impl<T> SlabMapModel<T> {
    fn new() -> Self {
        Self {
            items: SlabMap::new(),
            len: 0,
        }
    }

    fn item(&self, key: usize) -> &T {
        let item = &self.items[key];
        assert!(item.is_exists, "invalid slab map key");
        &item.value
    }

    fn insert(&mut self, value: T) -> (usize, ChangeData) {
        let key = self.items.insert(Item::new(value));
        self.len += 1;
        (
            key,
            ChangeData {
                action: ChangeAction::Insert,
                key,
            },
        )
    }

    fn remove(&mut self, key: usize) -> ChangeData {
        let item = &mut self.items[key];
        assert!(item.is_exists, "invalid slab map key");
        item.is_exists = false;
        self.len -= 1;
        ChangeData {
            action: ChangeAction::Remove,
            key,
        }
    }

    fn iter(&self) -> Iter<'_, T> {
        Iter(self.items.iter())
    }
}

impl<T: 'static> ChangeFeedModel for SlabMapModel<T> {
    type Change = ChangeData;

    fn release_change(&mut self, change: Self::Change) {
        if matches!(change.action, ChangeAction::Remove) {
            self.items.remove(change.key);
        }
    }
}

/// Iterates over live slab map entries.
pub struct Iter<'a, T>(slabmap::Iter<'a, Item<T>>);

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = (usize, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        for (key, value) in self.0.by_ref() {
            if value.is_exists {
                return Some((key, &value.value));
            }
        }
        None
    }
}

struct Item<T> {
    value: T,
    is_exists: bool,
}

impl<T> Item<T> {
    fn new(value: T) -> Self {
        Self {
            value,
            is_exists: true,
        }
    }
}

/// A single incremental slab map change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlabMapChange<'a, T> {
    /// An item was inserted.
    Insert { key: usize, new_value: &'a T },
    /// An item was removed.
    Remove { key: usize, old_value: &'a T },
}

#[derive(Clone, Copy)]
enum ChangeAction {
    Insert,
    Remove,
}

struct ChangeData {
    action: ChangeAction,
    key: usize,
}

/// Mutable reactive slab map state.
#[derive_ex(Default, Clone(bound()))]
#[default(Self::new())]
pub struct StateSlabMap<T: 'static>(Rc<RawStateSlabMap<T>>);

impl<T: 'static> StateSlabMap<T> {
    /// Creates an empty slab map.
    pub fn new() -> Self {
        Self(Rc::new(RawStateSlabMap {
            state: ChangeFeedState::new(SlabMapModel::new()),
            item_sinks: RefCell::new(ItemSinkBindings::new()),
        }))
    }

    /// Returns a signal backed by this state.
    pub fn to_signal(&self) -> SignalSlabMap<T> {
        SignalSlabMap(self.0.clone())
    }

    /// Borrows an item and registers a dependency on its key.
    ///
    /// # Panics
    ///
    /// Panics if `key` is not live.
    pub fn item<'a, 'r: 'a>(&'a self, key: usize, sc: &mut SignalContext<'r, '_>) -> Ref<'a, T> {
        let item = self.0.item(key);
        self.0.bind(key, sc);
        item
    }

    /// Borrows all current items and registers a collection-wide dependency.
    pub fn borrow<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> SlabMapRef<'a, T> {
        SlabMapRef::new(self.0.state.borrow(sc))
    }

    /// Inserts an item and returns its key.
    pub fn insert(&self, value: T, ac: &mut ActionContext) -> usize {
        let key = {
            let mut edit = self.0.state.borrow_mut(ac);
            let (key, change) = edit.current_mut().insert(value);
            edit.record(change);
            key
        };
        self.0.item_sinks.borrow_mut().notify(key, ac.nc());
        key
    }

    /// Removes an item.
    ///
    /// Reactive consumers notified by removal must not read `key` again.
    ///
    /// # Panics
    ///
    /// Panics if `key` is not live.
    pub fn remove(&self, key: usize, ac: &mut ActionContext) {
        {
            let mut edit = self.0.state.borrow_mut(ac);
            let change = edit.current_mut().remove(key);
            edit.record(change);
        }
        self.0.item_sinks.borrow_mut().notify(key, ac.nc());
    }

    /// Creates an independently advancing change reader.
    pub fn reader(&self) -> SignalSlabMapReader<T> {
        SignalSlabMapReader::from_state(self.0.state.reader())
    }
}

struct RawStateSlabMap<T: 'static> {
    state: ChangeFeedState<SlabMapModel<T>>,
    item_sinks: RefCell<ItemSinkBindings>,
}

impl<T: 'static> RawStateSlabMap<T> {
    fn rc_this(this: Rc<dyn Any>) -> Rc<Self> {
        Rc::downcast(this).unwrap()
    }

    fn bind(self: &Rc<Self>, key: usize, sc: &mut SignalContext<'_, '_>) {
        self.item_sinks.borrow_mut().bind(self.clone(), key, sc);
    }

    fn item(&self, key: usize) -> Ref<'_, T> {
        Ref::map(self.state.current_ref_untracked(), |model| model.item(key))
    }
}

impl<T: 'static> DynSignalSlabMap<T> for RawStateSlabMap<T> {
    fn to_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }

    fn item(&self, rc_self: Rc<dyn Any>, key: usize, sc: &mut SignalContext<'_, '_>) -> Ref<'_, T> {
        let item = self.item(key);
        Self::rc_this(rc_self).bind(key, sc);
        item
    }

    fn borrow<'a, 'r: 'a>(
        &'a self,
        _rc_self: Rc<dyn Any>,
        sc: &mut SignalContext<'r, '_>,
    ) -> SlabMapRef<'a, T> {
        SlabMapRef::new(self.state.borrow(sc))
    }

    fn watch_all(&self, _rc_self: Rc<dyn Any>, sc: &mut SignalContext<'_, '_>) {
        drop(self.state.borrow(sc));
    }

    fn reader(self: Rc<Self>) -> SignalSlabMapReader<T> {
        SignalSlabMapReader::from_state(self.state.reader())
    }
}

impl<T: 'static> BindSource for RawStateSlabMap<T> {
    fn check(self: Rc<Self>, slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) -> bool {
        self.item_sinks
            .borrow()
            .is_dirty(slot_to_key(slot).expect("item slot"), key, rc)
    }

    fn unbind(self: Rc<Self>, slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) {
        self.item_sinks
            .borrow_mut()
            .unbind(slot_to_key(slot).expect("item slot"), key, rc);
    }

    fn rebind(self: Rc<Self>, slot: Slot, key: BindKey, sc: &mut SignalContext<'_, '_>) {
        self.item_sinks.borrow_mut().rebind(
            self.clone(),
            slot_to_key(slot).expect("item slot"),
            key,
            sc,
        );
    }
}

struct ItemSinkBindings(Vec<SinkBindings>);

impl ItemSinkBindings {
    fn new() -> Self {
        Self(Vec::new())
    }

    fn bind(&mut self, this: Rc<dyn BindSource>, key: usize, sc: &mut SignalContext<'_, '_>) {
        if self.0.len() <= key {
            self.0.resize_with(key + 1, SinkBindings::new);
        }
        self.0[key].bind(this, key_to_slot(key), sc);
    }

    fn unbind(&mut self, item: usize, key: BindKey, rc: &mut ReactionContext<'_, '_>) {
        if let Some(sink) = self.0.get_mut(item) {
            sink.unbind(key, rc);
        }
    }

    fn rebind(
        &mut self,
        this: Rc<dyn BindSource>,
        item: usize,
        key: BindKey,
        sc: &mut SignalContext<'_, '_>,
    ) {
        if let Some(sink) = self.0.get_mut(item) {
            sink.rebind(this, key_to_slot(item), key, sc);
        }
    }

    fn is_dirty(&self, item: usize, key: BindKey, rc: &mut ReactionContext<'_, '_>) -> bool {
        self.0.get(item).is_none_or(|sink| sink.is_dirty(key, rc))
    }

    fn notify(&mut self, item: usize, nc: &mut NotifyContext) {
        if let Some(sink) = self.0.get_mut(item) {
            sink.notify(DirtyLevel::Dirty, nc);
        }
    }
}

struct SinkBindingsSet {
    items: Vec<SinkBindings>,
    any: SinkBindings,
}

impl SinkBindingsSet {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            any: SinkBindings::new(),
        }
    }

    fn update_changed(
        &mut self,
        keys: impl IntoIterator<Item = usize>,
        rc: &mut ReactionContext<'_, '_>,
    ) {
        let mut changed = false;
        for key in keys {
            changed = true;
            if let Some(sink) = self.items.get_mut(key) {
                sink.update(true, rc);
            }
        }
        if changed {
            self.any.update(true, rc);
        }
    }

    fn update_all(&mut self, is_dirty: bool, rc: &mut ReactionContext<'_, '_>) {
        for sink in &mut self.items {
            sink.update(is_dirty, rc);
        }
        self.any.update(is_dirty, rc);
    }

    fn notify_all(&mut self, level: DirtyLevel, nc: &mut NotifyContext) {
        for sink in &mut self.items {
            sink.notify(level, nc);
        }
        self.any.notify(level, nc);
    }

    fn bind(&mut self, this: Rc<dyn BindSource>, slot: Slot, sc: &mut SignalContext<'_, '_>) {
        if let Some(key) = slot_to_key(slot) {
            if self.items.len() <= key {
                self.items.resize_with(key + 1, SinkBindings::new);
            }
            self.items[key].bind(this, slot, sc);
        } else {
            self.any.bind(this, slot, sc);
        }
    }

    fn unbind(&mut self, slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) {
        if let Some(sink) = self.get_mut(slot) {
            sink.unbind(key, rc);
        }
    }

    fn rebind(
        &mut self,
        this: Rc<dyn BindSource>,
        slot: Slot,
        key: BindKey,
        sc: &mut SignalContext<'_, '_>,
    ) {
        if let Some(sink) = self.get_mut(slot) {
            sink.rebind(this, slot, key, sc);
        }
    }

    fn is_dirty(&self, slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) -> bool {
        self.get(slot).is_none_or(|sink| sink.is_dirty(key, rc))
    }

    fn get(&self, slot: Slot) -> Option<&SinkBindings> {
        if let Some(key) = slot_to_key(slot) {
            self.items.get(key)
        } else {
            Some(&self.any)
        }
    }

    fn get_mut(&mut self, slot: Slot) -> Option<&mut SinkBindings> {
        if let Some(key) = slot_to_key(slot) {
            self.items.get_mut(key)
        } else {
            Some(&mut self.any)
        }
    }
}

struct Scan<T: 'static, F> {
    storage: ChangeFeedStorage<SlabMapModel<T>>,
    data: RefCell<ScanData<F>>,
    sinks: RefCell<SinkBindingsSet>,
}

impl<T, F> Scan<T, F>
where
    T: 'static,
    F: FnMut(&mut SlabMapRefMut<'_, T>, &mut SignalContext<'_, '_>) + 'static,
{
    fn new(f: F) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            storage: ChangeFeedStorage::new(SlabMapModel::new()),
            data: RefCell::new(ScanData {
                source_binder: SourceBinder::new(this, Slot(0)),
                f,
            }),
            sinks: RefCell::new(SinkBindingsSet::new()),
        })
    }

    fn update(self: &Rc<Self>, rc: &mut ReactionContext<'_, '_>) {
        if rc.borrow(&self.data).source_binder.is_clean() {
            return;
        }
        let data = &mut *self.data.borrow_mut();
        if data.source_binder.check(rc) {
            let mut map = SlabMapRefMut {
                edit: self.storage.begin_edit(),
            };
            data.source_binder.update(|sc| (data.f)(&mut map, sc), rc);
            self.sinks
                .borrow_mut()
                .update_changed(map.edit.changes().map(|change| change.key), rc);
        }
        self.sinks.borrow_mut().update_all(false, rc);
    }

    fn rc_this(this: Rc<dyn Any>) -> Rc<Self> {
        Rc::downcast(this).unwrap()
    }

    fn watch(self: &Rc<Self>, slot: Slot, sc: &mut SignalContext<'_, '_>) {
        self.update(sc.rc());
        self.sinks.borrow_mut().bind(self.clone(), slot, sc);
    }
}

impl<T, F> DynSignalSlabMap<T> for Scan<T, F>
where
    T: 'static,
    F: FnMut(&mut SlabMapRefMut<'_, T>, &mut SignalContext<'_, '_>) + 'static,
{
    fn to_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }

    fn item(&self, rc_self: Rc<dyn Any>, key: usize, sc: &mut SignalContext<'_, '_>) -> Ref<'_, T> {
        let this = Self::rc_this(rc_self);
        this.update(sc.rc());
        let item = Ref::map(self.storage.current_ref(), |model| model.item(key));
        this.sinks
            .borrow_mut()
            .bind(this.clone(), key_to_slot(key), sc);
        item
    }

    fn borrow<'a, 'r: 'a>(
        &'a self,
        rc_self: Rc<dyn Any>,
        sc: &mut SignalContext<'r, '_>,
    ) -> SlabMapRef<'a, T> {
        Self::rc_this(rc_self).watch(SLOT_ITEMS, sc);
        SlabMapRef::new(self.storage.borrow_current())
    }

    fn watch_all(&self, rc_self: Rc<dyn Any>, sc: &mut SignalContext<'_, '_>) {
        Self::rc_this(rc_self).watch(SLOT_ITEMS, sc);
    }

    fn reader(self: Rc<Self>) -> SignalSlabMapReader<T> {
        let cursor = self.storage.reader();
        let source: Rc<dyn DynSignalSlabMap<T>> = self;
        SignalSlabMapReader::from_scan(source, cursor)
    }
}

impl<T, F> BindSource for Scan<T, F>
where
    T: 'static,
    F: FnMut(&mut SlabMapRefMut<'_, T>, &mut SignalContext<'_, '_>) + 'static,
{
    fn check(self: Rc<Self>, slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) -> bool {
        self.update(rc);
        self.sinks.borrow().is_dirty(slot, key, rc)
    }

    fn unbind(self: Rc<Self>, slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) {
        self.sinks.borrow_mut().unbind(slot, key, rc)
    }

    fn rebind(self: Rc<Self>, slot: Slot, key: BindKey, sc: &mut SignalContext<'_, '_>) {
        self.sinks.borrow_mut().rebind(self.clone(), slot, key, sc)
    }
}

impl<T, F> BindSink for Scan<T, F>
where
    T: 'static,
    F: FnMut(&mut SlabMapRefMut<'_, T>, &mut SignalContext<'_, '_>) + 'static,
{
    fn notify(self: Rc<Self>, slot: Slot, level: DirtyLevel, nc: &mut NotifyContext) {
        if self.data.borrow_mut().source_binder.on_notify(slot, level) {
            self.sinks
                .borrow_mut()
                .notify_all(DirtyLevel::MaybeDirty, nc);
        }
    }
}

struct ScanData<F> {
    source_binder: SourceBinder,
    f: F,
}

#[cfg(test)]
mod tests;
