use std::{
    any::Any,
    cell::{Ref, RefCell},
    mem,
    ops::Index,
    rc::Rc,
};

use derive_ex::derive_ex;
use slabmap::SlabMap;

use crate::{
    ActionContext, SignalContext,
    change_feed::{
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

pub struct SignalSlabMap<T>(Rc<dyn DynSignalSlabMap<T>>);

impl<T: 'static> SignalSlabMap<T> {
    pub fn from_scan(
        f: impl FnMut(&mut ItemsMut<T>, &mut SignalContext<'_, '_>) + 'static,
    ) -> Self {
        Self(Scan::new(f))
    }

    pub fn item<'a, 'r: 'a>(&'a self, key: usize, sc: &mut SignalContext<'r, '_>) -> Ref<'a, T> {
        self.0.item(self.0.clone().to_any(), key, sc)
    }

    pub fn items<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> Items<'a, T> {
        self.0.items(self.0.clone().to_any(), sc)
    }

    pub fn reader(&self) -> SignalSlabMapReader<T> {
        self.0.clone().reader()
    }
}

trait DynSignalSlabMap<T> {
    fn to_any(self: Rc<Self>) -> Rc<dyn Any>;
    fn item(&self, rc_self: Rc<dyn Any>, key: usize, sc: &mut SignalContext<'_, '_>) -> Ref<'_, T>;
    fn items<'a, 'r: 'a>(
        &'a self,
        rc_self: Rc<dyn Any>,
        sc: &mut SignalContext<'r, '_>,
    ) -> Items<'a, T>;
    fn watch_items(&self, rc_self: Rc<dyn Any>, sc: &mut SignalContext<'_, '_>);
    fn reader(self: Rc<Self>) -> SignalSlabMapReader<T>;
}

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

    pub fn read<'a, 'r: 'a>(&'a mut self, sc: &mut SignalContext<'r, '_>) -> Items<'a, T> {
        match &mut self.0 {
            RawSignalSlabMapReader::State(reader) => Items::new(reader.read(sc)),
            RawSignalSlabMapReader::Scan { source, cursor } => {
                source.watch_items(source.clone().to_any(), sc);
                Items::new(cursor.read())
            }
        }
    }

    pub fn peek<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> Items<'a, T> {
        match &self.0 {
            RawSignalSlabMapReader::State(reader) => Items::new(reader.peek(sc)),
            RawSignalSlabMapReader::Scan { source, cursor } => {
                source.watch_items(source.clone().to_any(), sc);
                Items::new(cursor.peek())
            }
        }
    }
}

pub struct Items<'a, T: 'static> {
    value: ChangeFeedRef<'a, SlabMapModel<T>>,
}

impl<'a, T: 'static> Items<'a, T> {
    fn new(value: ChangeFeedRef<'a, SlabMapModel<T>>) -> Self {
        Self { value }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn len(&self) -> usize {
        self.value.current().0.len
    }

    pub fn get(&self, key: usize) -> Option<&T> {
        self.value.current().0.get(key)
    }

    pub fn iter(&self) -> Iter<'_, T> {
        Iter(self.value.current().0.items.iter())
    }

    pub fn changes(&self) -> impl Iterator<Item = SlabMapChange<'_, T>> {
        use iter_n::iter2::*;
        match self.value.delta() {
            ChangeFeedDelta::Initial => self
                .iter()
                .map(|(key, new_value)| SlabMapChange::Insert { key, new_value })
                .into_iter0(),
            ChangeFeedDelta::Changes(changes) => changes
                .map(|change| {
                    let value = &self.value.current().0.items[change.key].value;
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

impl<T: 'static> Index<usize> for Items<'_, T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("index out of bounds")
    }
}

impl<'a, T: 'static> IntoIterator for &'a Items<'a, T> {
    type Item = (usize, &'a T);
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct ItemsMut<T> {
    items: SlabMap<Item<T>>,
    len: usize,
    pending_changes: Vec<ChangeData>,
}

impl<T> ItemsMut<T> {
    fn new() -> Self {
        Self {
            items: SlabMap::new(),
            len: 0,
            pending_changes: Vec::new(),
        }
    }

    fn take_changes(&mut self) -> Vec<ChangeData> {
        mem::take(&mut self.pending_changes)
    }

    pub fn get(&self, key: usize) -> Option<&T> {
        let item = self.items.get(key)?;
        if item.is_exists {
            Some(&item.value)
        } else {
            None
        }
    }

    pub fn insert(&mut self, value: T) -> usize {
        let key = self.items.insert(Item::new(value));
        self.len += 1;
        self.pending_changes.push(ChangeData {
            action: ChangeAction::Insert,
            key,
        });
        key
    }

    pub fn remove(&mut self, key: usize) {
        let item = &mut self.items[key];
        assert!(item.is_exists);
        item.is_exists = false;
        self.len -= 1;
        self.pending_changes.push(ChangeData {
            action: ChangeAction::Remove,
            key,
        });
    }
}

impl<T> Index<usize> for ItemsMut<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("index out of bounds")
    }
}

struct SlabMapModel<T>(ItemsMut<T>);

impl<T: 'static> ChangeFeedModel for SlabMapModel<T> {
    type Change = ChangeData;

    fn release_change(&mut self, change: Self::Change) {
        if matches!(change.action, ChangeAction::Remove) {
            self.0.items.remove(change.key);
        }
    }
}

fn record_pending<T: 'static>(edit: &mut ChangeFeedRefMut<'_, SlabMapModel<T>>) -> Vec<usize> {
    let mut keys = Vec::new();
    record_pending_into(edit, &mut keys);
    keys
}

fn record_pending_into<T: 'static>(
    edit: &mut ChangeFeedRefMut<'_, SlabMapModel<T>>,
    keys: &mut Vec<usize>,
) {
    let changes = edit.current_mut().0.take_changes();
    keys.extend(changes.iter().map(|change| change.key));
    for change in changes {
        edit.record(change);
    }
}

struct PendingEdit<'a, 'h, T: 'static> {
    edit: &'a mut ChangeFeedRefMut<'h, SlabMapModel<T>>,
    keys: &'a mut Vec<usize>,
}

impl<T: 'static> PendingEdit<'_, '_, T> {
    fn current(&mut self) -> &mut ItemsMut<T> {
        &mut self.edit.current_mut().0
    }
}

impl<T: 'static> Drop for PendingEdit<'_, '_, T> {
    fn drop(&mut self) {
        record_pending_into(self.edit, self.keys);
    }
}

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlabMapChange<'a, T> {
    Insert { key: usize, new_value: &'a T },
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

#[derive_ex(Default, Clone(bound()))]
#[default(Self::new())]
pub struct StateSlabMap<T: 'static>(Rc<RawStateSlabMap<T>>);

impl<T: 'static> StateSlabMap<T> {
    pub fn new() -> Self {
        Self(Rc::new(RawStateSlabMap {
            state: ChangeFeedState::new(SlabMapModel(ItemsMut::new())),
            item_sinks: RefCell::new(ItemSinkBindings::new()),
        }))
    }

    pub fn to_signal_slab_map(&self) -> SignalSlabMap<T> {
        SignalSlabMap(self.0.clone())
    }

    pub fn insert(&self, value: T, ac: &mut ActionContext) -> usize {
        let key;
        {
            let mut edit = self.0.state.borrow_mut(ac);
            key = edit.current_mut().0.insert(value);
            let keys = record_pending(&mut edit);
            debug_assert_eq!(keys, [key]);
        }
        self.0.item_sinks.borrow_mut().notify(key, ac.nc());
        key
    }

    pub fn remove(&self, key: usize, ac: &mut ActionContext) {
        {
            let mut edit = self.0.state.borrow_mut(ac);
            edit.current_mut().0.remove(key);
            let keys = record_pending(&mut edit);
            debug_assert_eq!(keys, [key]);
        }
        self.0.item_sinks.borrow_mut().notify(key, ac.nc());
    }

    pub fn item<'a, 'r: 'a>(&'a self, key: usize, sc: &mut SignalContext<'r, '_>) -> Ref<'a, T> {
        self.0.bind(key, sc);
        self.0.item(key)
    }

    pub fn items<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> Items<'a, T> {
        Items::new(self.0.state.borrow(sc))
    }

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
        Ref::map(self.state.current_ref_untracked(), |model| &model.0[key])
    }
}

impl<T: 'static> DynSignalSlabMap<T> for RawStateSlabMap<T> {
    fn to_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }

    fn item(&self, rc_self: Rc<dyn Any>, key: usize, sc: &mut SignalContext<'_, '_>) -> Ref<'_, T> {
        Self::rc_this(rc_self).bind(key, sc);
        self.item(key)
    }

    fn items<'a, 'r: 'a>(
        &'a self,
        _rc_self: Rc<dyn Any>,
        sc: &mut SignalContext<'r, '_>,
    ) -> Items<'a, T> {
        Items::new(self.state.borrow(sc))
    }

    fn watch_items(&self, _rc_self: Rc<dyn Any>, sc: &mut SignalContext<'_, '_>) {
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

    fn update_changed(&mut self, keys: &[usize], rc: &mut ReactionContext<'_, '_>) {
        for &key in keys {
            if let Some(sink) = self.items.get_mut(key) {
                sink.update(true, rc);
            }
        }
        if !keys.is_empty() {
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
    F: FnMut(&mut ItemsMut<T>, &mut SignalContext<'_, '_>) + 'static,
{
    fn new(f: F) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            storage: ChangeFeedStorage::new(SlabMapModel(ItemsMut::new())),
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
            let mut edit = self.storage.begin_edit();
            let mut keys = Vec::new();
            {
                let mut pending = PendingEdit {
                    edit: &mut edit,
                    keys: &mut keys,
                };
                data.source_binder
                    .update(|sc| (data.f)(pending.current(), sc), rc);
            }
            drop(edit);
            self.sinks.borrow_mut().update_changed(&keys, rc);
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
    F: FnMut(&mut ItemsMut<T>, &mut SignalContext<'_, '_>) + 'static,
{
    fn to_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }

    fn item(&self, rc_self: Rc<dyn Any>, key: usize, sc: &mut SignalContext<'_, '_>) -> Ref<'_, T> {
        Self::rc_this(rc_self).watch(key_to_slot(key), sc);
        Ref::map(self.storage.current_ref(), |model| &model.0[key])
    }

    fn items<'a, 'r: 'a>(
        &'a self,
        rc_self: Rc<dyn Any>,
        sc: &mut SignalContext<'r, '_>,
    ) -> Items<'a, T> {
        Self::rc_this(rc_self).watch(SLOT_ITEMS, sc);
        Items::new(self.storage.borrow_current())
    }

    fn watch_items(&self, rc_self: Rc<dyn Any>, sc: &mut SignalContext<'_, '_>) {
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
    F: FnMut(&mut ItemsMut<T>, &mut SignalContext<'_, '_>) + 'static,
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
    F: FnMut(&mut ItemsMut<T>, &mut SignalContext<'_, '_>) + 'static,
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
