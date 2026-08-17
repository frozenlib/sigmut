//! Reactive hash sets backed by the hash map change-feed implementation.

use std::{
    borrow::Borrow,
    collections::hash_map::RandomState,
    fmt::{self, Debug},
    hash::{BuildHasher, Hash},
    marker::PhantomData,
};

use derive_ex::derive_ex;
use serde::{Deserialize, Serialize};

use crate::{ActionContext, SignalContext};

use super::hash_map::{
    HashMapChange, RawItemsMut, SignalHashMap, SignalHashMapReader, StateHashMap,
};

/// A reactive read-only hash set.
#[derive_ex(Clone(bound()))]
pub struct SignalHashSet<T: 'static, S: 'static = RandomState>(SignalHashMap<T, (), S>);

impl<T> SignalHashSet<T, RandomState>
where
    T: Eq + Hash + 'static,
{
    /// Creates a derived hash set using [`RandomState`].
    pub fn from_scan(
        f: impl FnMut(&mut ItemsMut<'_, T>, &mut SignalContext<'_, '_>) + 'static,
    ) -> Self {
        Self::from_scan_with_hasher(RandomState::new(), f)
    }
}

impl<T, S> SignalHashSet<T, S>
where
    T: Eq + Hash + 'static,
    S: BuildHasher + 'static,
{
    /// Creates a derived hash set with the specified hash builder.
    pub fn from_scan_with_hasher(
        hash_builder: S,
        mut f: impl FnMut(&mut ItemsMut<'_, T, S>, &mut SignalContext<'_, '_>) + 'static,
    ) -> Self {
        Self(SignalHashMap::from_raw_scan(
            hash_builder,
            move |raw, sc| f(&mut ItemsMut { raw }, sc),
        ))
    }

    /// Returns whether a value exists and registers a dependency on its hash slot.
    pub fn contains<Q>(&self, value: &Q, sc: &mut SignalContext<'_, '_>) -> bool
    where
        T: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.0.get(value, sc).is_some()
    }

    /// Borrows all current values and registers a collection-wide dependency.
    pub fn items<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> Items<'a, T, S> {
        Items {
            map: self.0.items(sc),
        }
    }

    /// Creates an independently advancing change reader.
    pub fn reader(&self) -> SignalHashSetReader<T, S> {
        SignalHashSetReader(self.0.reader())
    }
}

/// An independently advancing reader for a [`SignalHashSet`].
#[derive_ex(Clone(bound()))]
pub struct SignalHashSetReader<T: 'static, S: 'static = RandomState>(SignalHashMapReader<T, (), S>);

impl<T: 'static, S: 'static> SignalHashSetReader<T, S> {
    /// Reads the current values and advances this reader's cursor.
    pub fn read<'a, 'r: 'a>(&'a mut self, sc: &mut SignalContext<'r, '_>) -> Items<'a, T, S> {
        Items {
            map: self.0.read(sc),
        }
    }

    /// Reads without advancing this reader's cursor.
    pub fn peek<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> Items<'a, T, S> {
        Items {
            map: self.0.peek(sc),
        }
    }
}

/// A borrowed view of the current hash set and its reader delta.
pub struct Items<'a, T: 'static, S: 'static = RandomState> {
    map: super::hash_map::Items<'a, T, (), S>,
}

impl<T: 'static, S: 'static> Items<'_, T, S> {
    /// Returns the number of values.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns whether the set contains no values.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Returns whether a borrowed value exists.
    pub fn contains<Q>(&self, value: &Q) -> bool
    where
        T: Borrow<Q> + Eq + Hash,
        Q: Eq + Hash + ?Sized,
        S: BuildHasher,
    {
        self.map.contains_key(value)
    }

    /// Iterates over current values in an unspecified order.
    pub fn iter(&self) -> Iter<'_, T> {
        Iter(self.map.iter())
    }

    /// Returns the delta needed to reproduce the current values from the reader baseline.
    pub fn delta(&self) -> impl Iterator<Item = HashSetChange<'_, T>> {
        self.map.delta().map(|change| match change {
            HashMapChange::Insert { key, .. } => HashSetChange::Insert { new_value: key },
            HashMapChange::Remove { key, .. } => HashSetChange::Remove { old_value: key },
            HashMapChange::Set { .. } => unreachable!("hash set does not replace values"),
        })
    }
}

impl<'a, T: 'static, S: 'static> IntoIterator for &'a Items<'a, T, S> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T: Debug, S> Debug for Items<'_, T, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

/// A change-recording mutable view of a [`StateHashSet`] or derived scan.
pub struct ItemsMut<'a, T: 'static, S: 'static = RandomState> {
    raw: RawItemsMut<'a, T, (), S>,
}

impl<T, S> ItemsMut<'_, T, S>
where
    T: Eq + Hash + 'static,
    S: BuildHasher + 'static,
{
    /// Returns the number of values.
    pub fn len(&self) -> usize {
        self.raw.len()
    }

    /// Returns whether the set contains no values.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns whether a borrowed value exists.
    pub fn contains<Q>(&self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.raw.get(value).is_some()
    }

    /// Inserts a value, returning whether it was newly inserted.
    pub fn insert(&mut self, value: T) -> bool {
        self.raw.insert_key(value, ())
    }

    /// Removes a value, returning whether it existed.
    pub fn remove<Q>(&mut self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.raw.remove(value)
    }

    /// Removes all values.
    pub fn clear(&mut self) {
        self.raw.clear();
    }

    /// Iterates over current values in an unspecified order.
    pub fn iter(&self) -> Iter<'_, T> {
        Iter(self.raw.iter())
    }
}

impl<'a, T, S> IntoIterator for &'a ItemsMut<'_, T, S>
where
    T: Eq + Hash,
    S: BuildHasher,
{
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T, S> Debug for ItemsMut<'_, T, S>
where
    T: Debug + Eq + Hash,
    S: BuildHasher,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

/// Iterates over borrowed hash set values.
pub struct Iter<'a, T>(super::hash_map::Iter<'a, T, ()>);

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(value, ())| value)
    }
}

/// A single incremental hash set change.
#[derive(Debug, Eq, PartialEq)]
#[derive_ex(Clone, Copy, bound())]
pub enum HashSetChange<'a, T: ?Sized> {
    /// A value was inserted.
    Insert { new_value: &'a T },
    /// A value was removed.
    Remove { old_value: &'a T },
}

/// Mutable reactive hash set state.
#[derive_ex(Clone(bound()))]
pub struct StateHashSet<T: 'static, S: 'static = RandomState>(StateHashMap<T, (), S>);

impl<T> StateHashSet<T, RandomState>
where
    T: Eq + Hash + 'static,
{
    /// Creates an empty hash set using [`RandomState`].
    pub fn new() -> Self {
        Self::with_hasher(RandomState::new())
    }
}

impl<T, S> StateHashSet<T, S>
where
    T: Eq + Hash + 'static,
    S: BuildHasher + 'static,
{
    /// Creates an empty hash set with the specified hash builder.
    pub fn with_hasher(hash_builder: S) -> Self {
        Self(StateHashMap::with_hasher(hash_builder))
    }

    /// Returns a signal backed by this state.
    pub fn to_signal_hash_set(&self) -> SignalHashSet<T, S> {
        SignalHashSet(self.0.to_signal_hash_map())
    }

    /// Inserts a value, returning whether it was newly inserted.
    pub fn insert(&self, value: T, ac: &mut ActionContext) -> bool {
        self.borrow_mut(ac).insert(value)
    }

    /// Removes a value, returning whether it existed.
    pub fn remove<Q>(&self, value: &Q, ac: &mut ActionContext) -> bool
    where
        T: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.borrow_mut(ac).remove(value)
    }

    /// Returns whether a value exists and registers a dependency on its hash slot.
    pub fn contains<Q>(&self, value: &Q, sc: &mut SignalContext<'_, '_>) -> bool
    where
        T: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.0.contains_key(value, sc)
    }

    /// Borrows all current values and registers a collection-wide dependency.
    pub fn items<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> Items<'a, T, S> {
        Items {
            map: self.0.items(sc),
        }
    }

    /// Mutably borrows the set for a change-recording edit.
    pub fn borrow_mut<'a>(&'a self, ac: &'a mut ActionContext) -> ItemsMut<'a, T, S> {
        ItemsMut {
            raw: self.0.borrow_mut_raw(ac),
        }
    }

    /// Creates an independently advancing change reader.
    pub fn reader(&self) -> SignalHashSetReader<T, S> {
        SignalHashSetReader(self.0.reader())
    }
}

impl<T> Default for StateHashSet<T, RandomState>
where
    T: Eq + Hash + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> FromIterator<T> for StateHashSet<T, RandomState>
where
    T: Eq + Hash + 'static,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self(StateHashMap::from_iter(
            iter.into_iter().map(|value| (value, ())),
        ))
    }
}

impl<T, S> Serialize for StateHashSet<T, S>
where
    T: Serialize + Eq + Hash + 'static,
    S: BuildHasher + 'static,
{
    fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
    where
        Ser: serde::Serializer,
    {
        let items = self
            .0
            .try_items_contextless()
            .map_err(serde::ser::Error::custom)?;
        serializer.collect_seq(items.iter().map(|(value, ())| value))
    }
}

impl<'de, T, S> Deserialize<'de> for StateHashSet<T, S>
where
    T: Deserialize<'de> + Eq + Hash + 'static,
    S: BuildHasher + Default + 'static,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct StateHashSetVisitor<T, S>(PhantomData<fn(T, S)>);

        impl<'de, T, S> serde::de::Visitor<'de> for StateHashSetVisitor<T, S>
        where
            T: Deserialize<'de> + Eq + Hash + 'static,
            S: BuildHasher + Default + 'static,
        {
            type Value = StateHashSet<T, S>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("sequence")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut values = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(value) = seq.next_element()? {
                    values.push((value, ()));
                }
                Ok(StateHashSet(StateHashMap::from_iter_with_hasher(
                    values,
                    S::default(),
                )))
            }
        }

        deserializer.deserialize_seq(StateHashSetVisitor(PhantomData))
    }
}

#[cfg(test)]
mod tests;
