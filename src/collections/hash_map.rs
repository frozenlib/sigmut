//! Reactive hash maps with collection-wide dependencies and incremental change readers.

use std::{
    collections::hash_map::RandomState,
    fmt::{self, Debug},
    hash::{BuildHasher, Hash},
    marker::PhantomData,
};

use derive_ex::derive_ex;
use serde::{Deserialize, Serialize};
use slabmap::SlabMap;

use crate::{
    ActionContext, SignalContext,
    building_blocks::change_feed::{
        ChangeFeedDelta, ChangeFeedModel, ChangeFeedReader, ChangeFeedRef, ChangeFeedRefMut,
        ChangeFeedSignal, ChangeFeedState,
    },
};

/// A reactive read-only hash map.
#[derive_ex(Clone(bound()))]
pub struct SignalHashMap<K: 'static, V: 'static, S: 'static = RandomState>(
    ChangeFeedSignal<HashMapModel<K, V, S>>,
);

impl<K, V> SignalHashMap<K, V, RandomState>
where
    K: Eq + Hash + 'static,
    V: 'static,
{
    /// Creates a derived hash map using [`RandomState`].
    pub fn from_scan(
        f: impl FnMut(&mut HashMapRefMut<'_, K, V>, &mut SignalContext<'_, '_>) + 'static,
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
        mut f: impl FnMut(&mut HashMapRefMut<'_, K, V, S>, &mut SignalContext<'_, '_>) + 'static,
    ) -> Self {
        Self(ChangeFeedSignal::from_scan(
            HashMapModel::new(hash_builder),
            move |edit, sc| {
                f(
                    &mut HashMapRefMut {
                        raw: RawHashMapRefMut::new(edit),
                    },
                    sc,
                );
            },
        ))
    }

    pub(crate) fn from_raw_scan(
        hash_builder: S,
        mut f: impl FnMut(RawHashMapRefMut<'_, K, V, S>, &mut SignalContext<'_, '_>) + 'static,
    ) -> Self {
        Self(ChangeFeedSignal::from_scan(
            HashMapModel::new(hash_builder),
            move |edit, sc| f(RawHashMapRefMut::new(edit), sc),
        ))
    }

    /// Borrows all current entries and registers a collection-wide dependency.
    pub fn borrow<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> HashMapRef<'a, K, V, S> {
        HashMapRef::new(self.0.borrow(sc))
    }

    /// Creates an independently advancing change reader.
    pub fn reader(&self) -> SignalHashMapReader<K, V, S> {
        SignalHashMapReader(self.0.reader())
    }
}

/// An independently advancing reader for a [`SignalHashMap`].
#[derive_ex(Clone(bound()))]
pub struct SignalHashMapReader<K: 'static, V: 'static, S: 'static = RandomState>(
    ChangeFeedReader<HashMapModel<K, V, S>>,
);

impl<K: 'static, V: 'static, S: 'static> SignalHashMapReader<K, V, S> {
    /// Reads the current entries and advances this reader's cursor.
    pub fn read<'a, 'r: 'a>(
        &'a mut self,
        sc: &mut SignalContext<'r, '_>,
    ) -> HashMapRef<'a, K, V, S> {
        HashMapRef::new(self.0.read(sc))
    }

    /// Reads without advancing this reader's cursor.
    pub fn peek<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> HashMapRef<'a, K, V, S> {
        HashMapRef::new(self.0.peek(sc))
    }
}

/// A borrowed reference to the current hash map and its reader delta.
pub struct HashMapRef<'a, K: 'static, V: 'static, S: 'static = RandomState> {
    value: ChangeFeedRef<'a, HashMapModel<K, V, S>>,
}

impl<'a, K: 'static, V: 'static, S: 'static> HashMapRef<'a, K, V, S> {
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

impl<'a, K: 'static, V: 'static, S: 'static> IntoIterator for &'a HashMapRef<'a, K, V, S> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<K, V, S> Debug for HashMapRef<'_, K, V, S>
where
    K: Debug,
    V: Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

/// A mutable borrowed reference that records hash map changes.
///
/// This guard exposes only operations that record their changes and deliberately does not
/// implement [`DerefMut`](std::ops::DerefMut).
pub struct HashMapRefMut<'a, K: 'static, V: 'static, S: 'static = RandomState> {
    raw: RawHashMapRefMut<'a, K, V, S>,
}

impl<K, V, S> HashMapRefMut<'_, K, V, S>
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

impl<'a, K, V, S> IntoIterator for &'a HashMapRefMut<'_, K, V, S>
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

impl<K, V, S> Debug for HashMapRefMut<'_, K, V, S>
where
    K: Debug + Eq + Hash,
    V: Debug,
    S: BuildHasher,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

pub(crate) struct RawHashMapRefMut<'a, K: 'static, V: 'static, S: 'static> {
    edit: ChangeFeedRefMut<'a, HashMapModel<K, V, S>>,
}

impl<'a, K: 'static, V: 'static, S: 'static> RawHashMapRefMut<'a, K, V, S> {
    fn new(edit: ChangeFeedRefMut<'a, HashMapModel<K, V, S>>) -> Self {
        Self { edit }
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
        self.edit.record(change);
        true
    }

    pub(crate) fn clear(&mut self)
    where
        K: Eq + Hash,
        S: BuildHasher,
    {
        for change in self.edit.current_mut().clear() {
            self.edit.record(change);
        }
    }

    pub(crate) fn iter(&self) -> Iter<'_, K, V> {
        self.edit.current().iter()
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
        if let Some(entry) = self.find_entry(hash, &key) {
            let new_value = self.values.insert(value);
            let old_value = std::mem::replace(&mut self.entries[entry].value, new_value);
            return (
                false,
                ChangeData::Set {
                    entry,
                    old_value,
                    new_value,
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
        (true, ChangeData::Insert { entry, new_value })
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
    },
    Remove {
        entry: usize,
        old_value: usize,
    },
    Set {
        entry: usize,
        old_value: usize,
        new_value: usize,
    },
}

impl ChangeData {
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
    ChangeFeedState<HashMapModel<K, V, S>>,
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
        Self(ChangeFeedState::new(model))
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
    pub fn to_signal(&self) -> SignalHashMap<K, V, S> {
        SignalHashMap(self.0.to_signal())
    }

    /// Borrows all current entries and registers a collection-wide dependency.
    pub fn borrow<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> HashMapRef<'a, K, V, S> {
        HashMapRef::new(self.0.borrow(sc))
    }

    pub(crate) fn try_borrow_contextless(
        &self,
    ) -> Result<HashMapRef<'_, K, V, S>, std::cell::BorrowError> {
        self.0.try_borrow_contextless().map(HashMapRef::new)
    }

    /// Mutably borrows the map for a change-recording edit.
    pub fn borrow_mut<'a>(&'a self, ac: &'a mut ActionContext) -> HashMapRefMut<'a, K, V, S> {
        HashMapRefMut {
            raw: RawHashMapRefMut::new(self.0.borrow_mut(ac)),
        }
    }

    /// Mutably borrows the map without tying the returned guard to the action context.
    ///
    /// Collection-wide notification is scheduled after this guard is dropped.
    pub fn borrow_mut_loose(&self, ac: &mut ActionContext) -> HashMapRefMut<'_, K, V, S> {
        HashMapRefMut {
            raw: RawHashMapRefMut::new(self.0.borrow_mut_loose(ac)),
        }
    }

    pub(crate) fn borrow_mut_raw<'a>(
        &'a self,
        ac: &'a mut ActionContext,
    ) -> RawHashMapRefMut<'a, K, V, S> {
        RawHashMapRefMut::new(self.0.borrow_mut(ac))
    }

    pub(crate) fn borrow_mut_loose_raw(
        &self,
        ac: &mut ActionContext,
    ) -> RawHashMapRefMut<'_, K, V, S> {
        RawHashMapRefMut::new(self.0.borrow_mut_loose(ac))
    }

    /// Creates an independently advancing change reader.
    pub fn reader(&self) -> SignalHashMapReader<K, V, S> {
        SignalHashMapReader(self.0.reader())
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

#[cfg(test)]
mod tests;
