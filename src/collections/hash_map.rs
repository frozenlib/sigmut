//! Reactive hash maps with key-specific dependencies and incremental change readers.

use std::{
    any::Any,
    cell::{Ref, RefCell},
    collections::hash_map::RandomState,
    fmt::{self, Debug},
    hash::{BuildHasher, Hash},
    marker::PhantomData,
    rc::Rc,
};

use derive_ex::derive_ex;
use serde::{Deserialize, Serialize};
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

fn hash_to_slot(hash: u64) -> Slot {
    let mut value = hash as usize;
    if usize::BITS < u64::BITS {
        value ^= (hash >> 32) as usize;
    }
    if value == SLOT_ITEMS.0 {
        value -= 1;
    }
    Slot(value)
}

/// A reactive read-only hash map.
#[derive_ex(Clone(bound()))]
pub struct SignalHashMap<K: 'static, V: 'static, S: 'static = RandomState>(
    Rc<dyn DynSignalHashMap<K, V, S>>,
);

impl<K, V> SignalHashMap<K, V, RandomState>
where
    K: Eq + Hash + 'static,
    V: 'static,
{
    /// Creates a derived hash map using [`RandomState`].
    pub fn from_scan(
        f: impl FnMut(&mut ItemsMut<'_, K, V>, &mut SignalContext<'_, '_>) + 'static,
    ) -> Self {
        Self::from_scan_with_hasher(RandomState::new(), f)
    }
}

impl<K, V, S> SignalHashMap<K, V, S>
where
    K: Eq + Hash + 'static,
    V: 'static,
    S: BuildHasher + 'static,
{
    /// Creates a derived hash map with the specified hash builder.
    pub fn from_scan_with_hasher(
        hash_builder: S,
        mut f: impl FnMut(&mut ItemsMut<'_, K, V, S>, &mut SignalContext<'_, '_>) + 'static,
    ) -> Self {
        Self::from_raw_scan(hash_builder, move |raw, sc| {
            f(&mut ItemsMut { raw }, sc);
        })
    }

    pub(crate) fn from_raw_scan(
        hash_builder: S,
        f: impl FnMut(RawItemsMut<'_, K, V, S>, &mut SignalContext<'_, '_>) + 'static,
    ) -> Self {
        Self(Scan::new(hash_builder, f))
    }

    /// Gets the value for a key and registers a dependency on its hash slot.
    pub fn get<'a, Q>(&'a self, key: &Q, sc: &mut SignalContext<'_, '_>) -> Option<Ref<'a, V>>
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        let slot = {
            let current = self.0.current_ref();
            hash_to_slot(current.hash(key))
        };
        self.0.watch_key(self.0.clone().to_any(), slot, sc);
        Ref::filter_map(self.0.current_ref(), |current| current.get(key)).ok()
    }

    /// Returns whether a key exists and registers a dependency on its hash slot.
    pub fn contains_key<Q>(&self, key: &Q, sc: &mut SignalContext<'_, '_>) -> bool
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.get(key, sc).is_some()
    }

    /// Borrows all current entries and registers a collection-wide dependency.
    pub fn items<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> Items<'a, K, V, S> {
        self.0.items(self.0.clone().to_any(), sc)
    }

    /// Creates an independently advancing change reader.
    pub fn reader(&self) -> SignalHashMapReader<K, V, S> {
        self.0.clone().reader()
    }
}

trait DynSignalHashMap<K: 'static, V: 'static, S: 'static> {
    fn to_any(self: Rc<Self>) -> Rc<dyn Any>;
    fn current_ref(&self) -> Ref<'_, HashMapModel<K, V, S>>;
    fn watch_key(&self, rc_self: Rc<dyn Any>, slot: Slot, sc: &mut SignalContext<'_, '_>);
    fn items<'a, 'r: 'a>(
        &'a self,
        rc_self: Rc<dyn Any>,
        sc: &mut SignalContext<'r, '_>,
    ) -> Items<'a, K, V, S>;
    fn watch_items(&self, rc_self: Rc<dyn Any>, sc: &mut SignalContext<'_, '_>);
    fn reader(self: Rc<Self>) -> SignalHashMapReader<K, V, S>;
}

/// An independently advancing reader for a [`SignalHashMap`].
#[derive_ex(Clone(bound()))]
pub struct SignalHashMapReader<K: 'static, V: 'static, S: 'static = RandomState>(
    RawSignalHashMapReader<K, V, S>,
);

#[derive_ex(Clone(bound()))]
enum RawSignalHashMapReader<K: 'static, V: 'static, S: 'static> {
    State(ChangeFeedReader<HashMapModel<K, V, S>>),
    Scan {
        source: Rc<dyn DynSignalHashMap<K, V, S>>,
        cursor: ChangeFeedCursorReader<HashMapModel<K, V, S>>,
    },
}

impl<K: 'static, V: 'static, S: 'static> SignalHashMapReader<K, V, S> {
    fn from_state(reader: ChangeFeedReader<HashMapModel<K, V, S>>) -> Self {
        Self(RawSignalHashMapReader::State(reader))
    }

    fn from_scan(
        source: Rc<dyn DynSignalHashMap<K, V, S>>,
        cursor: ChangeFeedCursorReader<HashMapModel<K, V, S>>,
    ) -> Self {
        Self(RawSignalHashMapReader::Scan { source, cursor })
    }

    /// Reads the current entries and advances this reader's cursor.
    pub fn read<'a, 'r: 'a>(&'a mut self, sc: &mut SignalContext<'r, '_>) -> Items<'a, K, V, S> {
        match &mut self.0 {
            RawSignalHashMapReader::State(reader) => Items::new(reader.read(sc)),
            RawSignalHashMapReader::Scan { source, cursor } => {
                source.watch_items(source.clone().to_any(), sc);
                Items::new(cursor.read())
            }
        }
    }

    /// Reads without advancing this reader's cursor.
    pub fn peek<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> Items<'a, K, V, S> {
        match &self.0 {
            RawSignalHashMapReader::State(reader) => Items::new(reader.peek(sc)),
            RawSignalHashMapReader::Scan { source, cursor } => {
                source.watch_items(source.clone().to_any(), sc);
                Items::new(cursor.peek())
            }
        }
    }
}

/// A borrowed view of the current hash map and its reader delta.
pub struct Items<'a, K: 'static, V: 'static, S: 'static = RandomState> {
    value: ChangeFeedRef<'a, HashMapModel<K, V, S>>,
}

impl<'a, K: 'static, V: 'static, S: 'static> Items<'a, K, V, S> {
    fn new(value: ChangeFeedRef<'a, HashMapModel<K, V, S>>) -> Self {
        Self { value }
    }

    /// Returns the number of entries.
    pub fn len(&self) -> usize {
        self.value.current().len()
    }

    /// Returns whether the map contains no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Gets a value by a borrowed form of its key.
    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: std::borrow::Borrow<Q> + Eq + Hash,
        Q: Eq + Hash + ?Sized,
        S: BuildHasher,
    {
        self.value.current().get(key)
    }

    /// Returns whether a borrowed key exists.
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: std::borrow::Borrow<Q> + Eq + Hash,
        Q: Eq + Hash + ?Sized,
        S: BuildHasher,
    {
        self.get(key).is_some()
    }

    /// Iterates over current entries in an unspecified order.
    pub fn iter(&self) -> Iter<'_, K, V> {
        self.value.current().iter()
    }

    /// Returns the delta needed to reproduce the current entries from the reader baseline.
    pub fn delta(&self) -> impl Iterator<Item = HashMapChange<'_, K, V>> {
        use iter_n::iter2::*;
        match self.value.delta() {
            ChangeFeedDelta::Initial => self
                .iter()
                .map(|(key, new_value)| HashMapChange::Insert { key, new_value })
                .into_iter0(),
            ChangeFeedDelta::Incremental(changes) => changes
                .map(|change| change.to_hash_map_change(self.value.current()))
                .into_iter1(),
        }
    }
}

impl<'a, K: 'static, V: 'static, S: 'static> IntoIterator for &'a Items<'a, K, V, S> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<K, V, S> Debug for Items<'_, K, V, S>
where
    K: Debug,
    V: Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

/// A change-recording mutable view of a [`StateHashMap`] or derived scan.
pub struct ItemsMut<'a, K: 'static, V: 'static, S: 'static = RandomState> {
    raw: RawItemsMut<'a, K, V, S>,
}

impl<K, V, S> ItemsMut<'_, K, V, S>
where
    K: Eq + Hash + 'static,
    V: 'static,
    S: BuildHasher + 'static,
{
    /// Returns the number of entries.
    pub fn len(&self) -> usize {
        self.raw.len()
    }

    /// Returns whether the map contains no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Gets a value by a borrowed form of its key.
    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.raw.get(key)
    }

    /// Returns whether a borrowed key exists.
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.get(key).is_some()
    }

    /// Inserts a key-value pair, returning whether the key was newly inserted.
    pub fn insert(&mut self, key: K, value: V) -> bool {
        self.raw.insert(key, value)
    }

    /// Removes a key, returning whether an entry existed.
    pub fn remove<Q>(&mut self, key: &Q) -> bool
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.raw.remove(key)
    }

    /// Removes all entries.
    pub fn clear(&mut self) {
        self.raw.clear();
    }

    /// Iterates over current entries in an unspecified order.
    pub fn iter(&self) -> Iter<'_, K, V> {
        self.raw.iter()
    }
}

impl<'a, K, V, S> IntoIterator for &'a ItemsMut<'_, K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<K, V, S> Debug for ItemsMut<'_, K, V, S>
where
    K: Debug + Eq + Hash,
    V: Debug,
    S: BuildHasher,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

pub(crate) struct RawItemsMut<'a, K: 'static, V: 'static, S: 'static> {
    edit: ChangeFeedRefMut<'a, HashMapModel<K, V, S>>,
    changed_slots: Rc<RefCell<Vec<Slot>>>,
    item_sinks: Option<&'a RefCell<HashSlotBindings>>,
}

impl<'a, K: 'static, V: 'static, S: 'static> RawItemsMut<'a, K, V, S> {
    fn new(
        edit: ChangeFeedRefMut<'a, HashMapModel<K, V, S>>,
        changed_slots: Rc<RefCell<Vec<Slot>>>,
        item_sinks: Option<&'a RefCell<HashSlotBindings>>,
    ) -> Self {
        Self {
            edit,
            changed_slots,
            item_sinks,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.edit.current().len()
    }

    pub(crate) fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: std::borrow::Borrow<Q> + Eq + Hash,
        Q: Eq + Hash + ?Sized,
        S: BuildHasher,
    {
        self.edit.current().get(key)
    }

    pub(crate) fn insert(&mut self, key: K, value: V) -> bool
    where
        K: Eq + Hash,
        S: BuildHasher,
    {
        let (inserted, change) = self.edit.current_mut().insert(key, value);
        self.changed_slots.borrow_mut().push(change.slot());
        self.edit.record(change);
        inserted
    }

    pub(crate) fn insert_key(&mut self, key: K, value: V) -> bool
    where
        K: Eq + Hash,
        S: BuildHasher,
    {
        if self.edit.current().contains_key(&key) {
            return false;
        }
        debug_assert!(self.insert(key, value));
        true
    }

    pub(crate) fn remove<Q>(&mut self, key: &Q) -> bool
    where
        K: std::borrow::Borrow<Q> + Eq + Hash,
        Q: Eq + Hash + ?Sized,
        S: BuildHasher,
    {
        let Some(change) = self.edit.current_mut().remove(key) else {
            return false;
        };
        self.changed_slots.borrow_mut().push(change.slot());
        self.edit.record(change);
        true
    }

    pub(crate) fn clear(&mut self)
    where
        K: Eq + Hash,
        S: BuildHasher,
    {
        for change in self.edit.current_mut().clear() {
            self.changed_slots.borrow_mut().push(change.slot());
            self.edit.record(change);
        }
    }

    pub(crate) fn iter(&self) -> Iter<'_, K, V> {
        self.edit.current().iter()
    }
}

impl<K: 'static, V: 'static, S: 'static> Drop for RawItemsMut<'_, K, V, S> {
    fn drop(&mut self) {
        let Some(item_sinks) = self.item_sinks else {
            return;
        };
        let changed_slots = RefCell::borrow(&self.changed_slots);
        self.edit.with_notify_context(|nc| {
            item_sinks.borrow_mut().notify(&changed_slots, nc);
        });
    }
}

/// Iterates over borrowed hash map entries.
pub struct Iter<'a, K, V> {
    entries: slabmap::Iter<'a, Entry<K>>,
    values: &'a SlabMap<V>,
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        for (_, entry) in self.entries.by_ref() {
            if entry.exists {
                return Some((&entry.key, &self.values[entry.value]));
            }
        }
        None
    }
}

/// A single incremental hash map change.
#[derive(Debug, Eq, PartialEq)]
#[derive_ex(Clone, Copy, bound())]
pub enum HashMapChange<'a, K: ?Sized, V: ?Sized> {
    /// A new key-value pair was inserted.
    Insert { key: &'a K, new_value: &'a V },
    /// A key-value pair was removed.
    Remove { key: &'a K, old_value: &'a V },
    /// The value for an existing key was replaced.
    Set {
        key: &'a K,
        old_value: &'a V,
        new_value: &'a V,
    },
}

struct HashMapModel<K, V, S> {
    entries: SlabMap<Entry<K>>,
    values: SlabMap<V>,
    index: std::collections::HashMap<u64, Vec<usize>>,
    hash_builder: S,
    len: usize,
}

impl<K, V, S> HashMapModel<K, V, S> {
    fn new(hash_builder: S) -> Self {
        Self {
            entries: SlabMap::new(),
            values: SlabMap::new(),
            index: std::collections::HashMap::new(),
            hash_builder,
            len: 0,
        }
    }

    fn len(&self) -> usize {
        self.len
    }

    fn iter(&self) -> Iter<'_, K, V> {
        Iter {
            entries: self.entries.iter(),
            values: &self.values,
        }
    }
}

impl<K, V, S> HashMapModel<K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    fn hash<Q: Hash + ?Sized>(&self, key: &Q) -> u64 {
        self.hash_builder.hash_one(key)
    }

    fn find_entry<Q>(&self, hash: u64, key: &Q) -> Option<usize>
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + ?Sized,
    {
        self.index.get(&hash)?.iter().copied().find(|&entry| {
            let entry = &self.entries[entry];
            entry.exists && <K as std::borrow::Borrow<Q>>::borrow(&entry.key) == key
        })
    }

    fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        let entry = &self.entries[self.find_entry(self.hash(key), key)?];
        Some(&self.values[entry.value])
    }

    fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.get(key).is_some()
    }

    fn insert(&mut self, key: K, value: V) -> (bool, ChangeData) {
        let hash = self.hash(&key);
        let slot = hash_to_slot(hash);
        if let Some(entry) = self.find_entry(hash, &key) {
            let new_value = self.values.insert(value);
            let old_value = std::mem::replace(&mut self.entries[entry].value, new_value);
            return (
                false,
                ChangeData::Set {
                    entry,
                    old_value,
                    new_value,
                    slot,
                },
            );
        }

        let new_value = self.values.insert(value);
        let entry = self.entries.insert(Entry {
            key,
            value: new_value,
            hash,
            exists: true,
        });
        self.index.entry(hash).or_default().push(entry);
        self.len += 1;
        (
            true,
            ChangeData::Insert {
                entry,
                new_value,
                slot,
            },
        )
    }

    fn remove<Q>(&mut self, key: &Q) -> Option<ChangeData>
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        let hash = self.hash(key);
        let entry = self.find_entry(hash, key)?;
        Some(self.remove_entry(entry))
    }

    fn remove_entry(&mut self, entry: usize) -> ChangeData {
        let hash = self.entries[entry].hash;
        let bucket = self.index.get_mut(&hash).expect("active entry bucket");
        let index = bucket
            .iter()
            .position(|&candidate| candidate == entry)
            .expect("active entry index");
        bucket.swap_remove(index);
        if bucket.is_empty() {
            self.index.remove(&hash);
        }
        let item = &mut self.entries[entry];
        debug_assert!(item.exists);
        item.exists = false;
        self.len -= 1;
        ChangeData::Remove {
            entry,
            old_value: item.value,
            slot: hash_to_slot(hash),
        }
    }

    fn clear(&mut self) -> Vec<ChangeData> {
        let entries: Vec<_> = self
            .entries
            .iter()
            .filter_map(|(entry, value)| value.exists.then_some(entry))
            .collect();
        entries
            .into_iter()
            .map(|entry| self.remove_entry(entry))
            .collect()
    }
}

impl<K: 'static, V: 'static, S: 'static> ChangeFeedModel for HashMapModel<K, V, S> {
    type Change = ChangeData;

    fn release_change(&mut self, change: Self::Change) {
        match change {
            ChangeData::Insert { .. } => {}
            ChangeData::Set { old_value, .. } => {
                self.values.remove(old_value);
            }
            ChangeData::Remove {
                entry, old_value, ..
            } => {
                self.values.remove(old_value);
                self.entries.remove(entry);
            }
        }
    }
}

struct Entry<K> {
    key: K,
    value: usize,
    hash: u64,
    exists: bool,
}

enum ChangeData {
    Insert {
        entry: usize,
        new_value: usize,
        slot: Slot,
    },
    Remove {
        entry: usize,
        old_value: usize,
        slot: Slot,
    },
    Set {
        entry: usize,
        old_value: usize,
        new_value: usize,
        slot: Slot,
    },
}

impl ChangeData {
    fn slot(&self) -> Slot {
        match *self {
            Self::Insert { slot, .. } | Self::Remove { slot, .. } | Self::Set { slot, .. } => slot,
        }
    }

    fn to_hash_map_change<'a, K, V, S>(
        &'a self,
        model: &'a HashMapModel<K, V, S>,
    ) -> HashMapChange<'a, K, V> {
        match *self {
            Self::Insert {
                entry, new_value, ..
            } => HashMapChange::Insert {
                key: &model.entries[entry].key,
                new_value: &model.values[new_value],
            },
            Self::Remove {
                entry, old_value, ..
            } => HashMapChange::Remove {
                key: &model.entries[entry].key,
                old_value: &model.values[old_value],
            },
            Self::Set {
                entry,
                old_value,
                new_value,
                ..
            } => HashMapChange::Set {
                key: &model.entries[entry].key,
                old_value: &model.values[old_value],
                new_value: &model.values[new_value],
            },
        }
    }
}

/// Mutable reactive hash map state.
#[derive_ex(Clone(bound()))]
pub struct StateHashMap<K: 'static, V: 'static, S: 'static = RandomState>(
    Rc<RawStateHashMap<K, V, S>>,
);

impl<K, V> StateHashMap<K, V, RandomState>
where
    K: Eq + Hash + 'static,
    V: 'static,
{
    /// Creates an empty hash map using [`RandomState`].
    pub fn new() -> Self {
        Self::with_hasher(RandomState::new())
    }
}

impl<K, V, S> StateHashMap<K, V, S>
where
    K: Eq + Hash + 'static,
    V: 'static,
    S: BuildHasher + 'static,
{
    /// Creates an empty hash map with the specified hash builder.
    pub fn with_hasher(hash_builder: S) -> Self {
        Self::from_model(HashMapModel::new(hash_builder))
    }

    fn from_model(model: HashMapModel<K, V, S>) -> Self {
        Self(Rc::new(RawStateHashMap {
            state: ChangeFeedState::new(model),
            item_sinks: RefCell::new(HashSlotBindings::new()),
        }))
    }

    pub(crate) fn from_iter_with_hasher(
        iter: impl IntoIterator<Item = (K, V)>,
        hash_builder: S,
    ) -> Self {
        let mut model = HashMapModel::new(hash_builder);
        for (key, value) in iter {
            let (_, change) = model.insert(key, value);
            model.release_change(change);
        }
        Self::from_model(model)
    }

    /// Returns a signal backed by this state.
    pub fn to_signal_hash_map(&self) -> SignalHashMap<K, V, S> {
        SignalHashMap(self.0.clone())
    }

    /// Inserts a key-value pair, returning whether the key was newly inserted.
    pub fn insert(&self, key: K, value: V, ac: &mut ActionContext) -> bool {
        self.borrow_mut(ac).insert(key, value)
    }

    /// Removes a key, returning whether an entry existed.
    pub fn remove<Q>(&self, key: &Q, ac: &mut ActionContext) -> bool
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.borrow_mut(ac).remove(key)
    }

    /// Gets the value for a key and registers a dependency on its hash slot.
    pub fn get<'a, Q>(&'a self, key: &Q, sc: &mut SignalContext<'_, '_>) -> Option<Ref<'a, V>>
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.0.get(key, sc)
    }

    /// Returns whether a key exists and registers a dependency on its hash slot.
    pub fn contains_key<Q>(&self, key: &Q, sc: &mut SignalContext<'_, '_>) -> bool
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.get(key, sc).is_some()
    }

    /// Borrows all current entries and registers a collection-wide dependency.
    pub fn items<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> Items<'a, K, V, S> {
        Items::new(self.0.state.borrow(sc))
    }

    pub(crate) fn try_items_contextless(
        &self,
    ) -> Result<Items<'_, K, V, S>, std::cell::BorrowError> {
        self.0.state.try_borrow_contextless().map(Items::new)
    }

    /// Mutably borrows the map for a change-recording edit.
    pub fn borrow_mut<'a>(&'a self, ac: &'a mut ActionContext) -> ItemsMut<'a, K, V, S> {
        ItemsMut {
            raw: self.borrow_mut_raw(ac),
        }
    }

    pub(crate) fn borrow_mut_raw<'a>(
        &'a self,
        ac: &'a mut ActionContext,
    ) -> RawItemsMut<'a, K, V, S> {
        RawItemsMut::new(
            self.0.state.borrow_mut(ac),
            Rc::new(RefCell::new(Vec::new())),
            Some(&self.0.item_sinks),
        )
    }

    /// Creates an independently advancing change reader.
    pub fn reader(&self) -> SignalHashMapReader<K, V, S> {
        SignalHashMapReader::from_state(self.0.state.reader())
    }
}

impl<K, V> Default for StateHashMap<K, V, RandomState>
where
    K: Eq + Hash + 'static,
    V: 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> FromIterator<(K, V)> for StateHashMap<K, V, RandomState>
where
    K: Eq + Hash + 'static,
    V: 'static,
{
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        Self::from_iter_with_hasher(iter, RandomState::new())
    }
}

impl<K, V, S> Serialize for StateHashMap<K, V, S>
where
    K: Serialize + 'static,
    V: Serialize + 'static,
    S: 'static,
{
    fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
    where
        Ser: serde::Serializer,
    {
        let items = self
            .0
            .state
            .try_borrow_contextless()
            .map_err(serde::ser::Error::custom)?;
        serializer.collect_map(items.current().iter())
    }
}

impl<'de, K, V, S> Deserialize<'de> for StateHashMap<K, V, S>
where
    K: Deserialize<'de> + Eq + Hash + 'static,
    V: Deserialize<'de> + 'static,
    S: BuildHasher + Default + 'static,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct StateHashMapVisitor<K, V, S>(PhantomData<fn(K, V, S)>);

        impl<'de, K, V, S> serde::de::Visitor<'de> for StateHashMapVisitor<K, V, S>
        where
            K: Deserialize<'de> + Eq + Hash + 'static,
            V: Deserialize<'de> + 'static,
            S: BuildHasher + Default + 'static,
        {
            type Value = StateHashMap<K, V, S>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("map")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut values = Vec::with_capacity(map.size_hint().unwrap_or(0));
                while let Some(value) = map.next_entry()? {
                    values.push(value);
                }
                Ok(StateHashMap::from_iter_with_hasher(values, S::default()))
            }
        }

        deserializer.deserialize_map(StateHashMapVisitor(PhantomData))
    }
}

struct RawStateHashMap<K: 'static, V: 'static, S: 'static> {
    state: ChangeFeedState<HashMapModel<K, V, S>>,
    item_sinks: RefCell<HashSlotBindings>,
}

impl<K, V, S> RawStateHashMap<K, V, S>
where
    K: Eq + Hash + 'static,
    V: 'static,
    S: BuildHasher + 'static,
{
    fn rc_this(this: Rc<dyn Any>) -> Rc<Self> {
        Rc::downcast(this).unwrap()
    }

    fn bind(self: &Rc<Self>, slot: Slot, sc: &mut SignalContext<'_, '_>) {
        self.item_sinks.borrow_mut().bind(self.clone(), slot, sc);
    }

    fn get<'a, Q>(self: &'a Rc<Self>, key: &Q, sc: &mut SignalContext<'_, '_>) -> Option<Ref<'a, V>>
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        let slot = {
            let current = self.state.current_ref_untracked();
            hash_to_slot(current.hash(key))
        };
        self.bind(slot, sc);
        Ref::filter_map(self.state.current_ref_untracked(), |current| {
            current.get(key)
        })
        .ok()
    }
}

impl<K, V, S> DynSignalHashMap<K, V, S> for RawStateHashMap<K, V, S>
where
    K: Eq + Hash + 'static,
    V: 'static,
    S: BuildHasher + 'static,
{
    fn to_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }

    fn current_ref(&self) -> Ref<'_, HashMapModel<K, V, S>> {
        self.state.current_ref_untracked()
    }

    fn watch_key(&self, rc_self: Rc<dyn Any>, slot: Slot, sc: &mut SignalContext<'_, '_>) {
        Self::rc_this(rc_self).bind(slot, sc);
    }

    fn items<'a, 'r: 'a>(
        &'a self,
        _rc_self: Rc<dyn Any>,
        sc: &mut SignalContext<'r, '_>,
    ) -> Items<'a, K, V, S> {
        Items::new(self.state.borrow(sc))
    }

    fn watch_items(&self, _rc_self: Rc<dyn Any>, sc: &mut SignalContext<'_, '_>) {
        drop(self.state.borrow(sc));
    }

    fn reader(self: Rc<Self>) -> SignalHashMapReader<K, V, S> {
        SignalHashMapReader::from_state(self.state.reader())
    }
}

impl<K: 'static, V: 'static, S: 'static> BindSource for RawStateHashMap<K, V, S> {
    fn check(self: Rc<Self>, slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) -> bool {
        self.item_sinks.borrow().is_dirty(slot, key, rc)
    }

    fn unbind(self: Rc<Self>, slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) {
        self.item_sinks.borrow_mut().unbind(slot, key, rc);
    }

    fn rebind(self: Rc<Self>, slot: Slot, key: BindKey, sc: &mut SignalContext<'_, '_>) {
        self.item_sinks
            .borrow_mut()
            .rebind(self.clone(), slot, key, sc);
    }
}

struct HashSlotBindings(std::collections::HashMap<usize, SinkBindings>);

impl HashSlotBindings {
    fn new() -> Self {
        Self(std::collections::HashMap::new())
    }

    fn bind(&mut self, this: Rc<dyn BindSource>, slot: Slot, sc: &mut SignalContext<'_, '_>) {
        self.0.entry(slot.0).or_default().bind(this, slot, sc);
    }

    fn unbind(&mut self, slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) {
        if let Some(sink) = self.0.get_mut(&slot.0) {
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
        if let Some(sink) = self.0.get_mut(&slot.0) {
            sink.rebind(this, slot, key, sc);
        }
    }

    fn is_dirty(&self, slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) -> bool {
        self.0
            .get(&slot.0)
            .is_none_or(|sink| sink.is_dirty(key, rc))
    }

    fn notify(&mut self, slots: &[Slot], nc: &mut NotifyContext) {
        for slot in slots {
            if let Some(sink) = self.0.get_mut(&slot.0) {
                sink.notify(DirtyLevel::Dirty, nc);
            }
        }
    }
}

struct SinkBindingsSet {
    keys: std::collections::HashMap<usize, SinkBindings>,
    items: SinkBindings,
}

impl SinkBindingsSet {
    fn new() -> Self {
        Self {
            keys: std::collections::HashMap::new(),
            items: SinkBindings::new(),
        }
    }

    fn update_changed(&mut self, slots: &[Slot], rc: &mut ReactionContext<'_, '_>) {
        for slot in slots {
            if let Some(sink) = self.keys.get_mut(&slot.0) {
                sink.update(true, rc);
            }
        }
        if !slots.is_empty() {
            self.items.update(true, rc);
        }
    }

    fn update_all(&mut self, is_dirty: bool, rc: &mut ReactionContext<'_, '_>) {
        for sink in self.keys.values_mut() {
            sink.update(is_dirty, rc);
        }
        self.items.update(is_dirty, rc);
    }

    fn notify_all(&mut self, level: DirtyLevel, nc: &mut NotifyContext) {
        for sink in self.keys.values_mut() {
            sink.notify(level, nc);
        }
        self.items.notify(level, nc);
    }

    fn bind(&mut self, this: Rc<dyn BindSource>, slot: Slot, sc: &mut SignalContext<'_, '_>) {
        if slot == SLOT_ITEMS {
            self.items.bind(this, slot, sc);
        } else {
            self.keys.entry(slot.0).or_default().bind(this, slot, sc);
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
        if slot == SLOT_ITEMS {
            self.items.is_dirty(key, rc)
        } else {
            self.keys
                .get(&slot.0)
                .is_none_or(|sink| sink.is_dirty(key, rc))
        }
    }

    fn get_mut(&mut self, slot: Slot) -> Option<&mut SinkBindings> {
        if slot == SLOT_ITEMS {
            Some(&mut self.items)
        } else {
            self.keys.get_mut(&slot.0)
        }
    }
}

struct Scan<K: 'static, V: 'static, S: 'static, F> {
    storage: ChangeFeedStorage<HashMapModel<K, V, S>>,
    data: RefCell<ScanData<F>>,
    sinks: RefCell<SinkBindingsSet>,
}

impl<K, V, S, F> Scan<K, V, S, F>
where
    K: Eq + Hash + 'static,
    V: 'static,
    S: BuildHasher + 'static,
    F: FnMut(RawItemsMut<'_, K, V, S>, &mut SignalContext<'_, '_>) + 'static,
{
    fn new(hash_builder: S, f: F) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            storage: ChangeFeedStorage::new(HashMapModel::new(hash_builder)),
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
            let changed_slots = Rc::new(RefCell::new(Vec::new()));
            let edit = self.storage.begin_edit();
            data.source_binder.update(
                {
                    let changed_slots = changed_slots.clone();
                    |sc| (data.f)(RawItemsMut::new(edit, changed_slots, None), sc)
                },
                rc,
            );
            self.sinks
                .borrow_mut()
                .update_changed(&RefCell::borrow(&changed_slots), rc);
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

impl<K, V, S, F> DynSignalHashMap<K, V, S> for Scan<K, V, S, F>
where
    K: Eq + Hash + 'static,
    V: 'static,
    S: BuildHasher + 'static,
    F: FnMut(RawItemsMut<'_, K, V, S>, &mut SignalContext<'_, '_>) + 'static,
{
    fn to_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }

    fn current_ref(&self) -> Ref<'_, HashMapModel<K, V, S>> {
        self.storage.current_ref()
    }

    fn watch_key(&self, rc_self: Rc<dyn Any>, slot: Slot, sc: &mut SignalContext<'_, '_>) {
        Self::rc_this(rc_self).watch(slot, sc);
    }

    fn items<'a, 'r: 'a>(
        &'a self,
        rc_self: Rc<dyn Any>,
        sc: &mut SignalContext<'r, '_>,
    ) -> Items<'a, K, V, S> {
        Self::rc_this(rc_self).watch(SLOT_ITEMS, sc);
        Items::new(self.storage.borrow_current())
    }

    fn watch_items(&self, rc_self: Rc<dyn Any>, sc: &mut SignalContext<'_, '_>) {
        Self::rc_this(rc_self).watch(SLOT_ITEMS, sc);
    }

    fn reader(self: Rc<Self>) -> SignalHashMapReader<K, V, S> {
        let cursor = self.storage.reader();
        let source: Rc<dyn DynSignalHashMap<K, V, S>> = self;
        SignalHashMapReader::from_scan(source, cursor)
    }
}

impl<K, V, S, F> BindSource for Scan<K, V, S, F>
where
    K: Eq + Hash + 'static,
    V: 'static,
    S: BuildHasher + 'static,
    F: FnMut(RawItemsMut<'_, K, V, S>, &mut SignalContext<'_, '_>) + 'static,
{
    fn check(self: Rc<Self>, slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) -> bool {
        self.update(rc);
        self.sinks.borrow().is_dirty(slot, key, rc)
    }

    fn unbind(self: Rc<Self>, slot: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) {
        self.sinks.borrow_mut().unbind(slot, key, rc);
    }

    fn rebind(self: Rc<Self>, slot: Slot, key: BindKey, sc: &mut SignalContext<'_, '_>) {
        self.sinks.borrow_mut().rebind(self.clone(), slot, key, sc);
    }
}

impl<K, V, S, F> BindSink for Scan<K, V, S, F>
where
    K: Eq + Hash + 'static,
    V: 'static,
    S: BuildHasher + 'static,
    F: FnMut(RawItemsMut<'_, K, V, S>, &mut SignalContext<'_, '_>) + 'static,
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
