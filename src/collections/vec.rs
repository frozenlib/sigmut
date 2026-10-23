//! Reactive vectors with incremental change readers.

use std::{
    cmp::Ordering,
    fmt::{self, Debug},
    marker::PhantomData,
    ops::{Deref, DerefMut, Index, RangeBounds},
    rc::Rc,
    sync::Arc,
};

use derive_ex::{Ex, derive_ex};
use serde::{Deserialize, Serialize};
use slabmap::SlabMap;

use crate::{
    ActionContext, SignalContext,
    building_blocks::change_feed::{
        ChangeFeedDelta, ChangeFeedModel, ChangeFeedReader, ChangeFeedRef, ChangeFeedRefMut,
        ChangeFeedSignal, ChangeFeedState,
    },
    utils::{IndexNewToOld, IndexNewToOldBuf, to_range},
};

#[derive(Ex)]
#[derive_ex(Clone(bound()))]
/// A reactive read-only vector.
pub struct SignalVec<T: 'static>(RawSignalVec<T>);

impl<T: 'static> SignalVec<T> {
    /// Creates a derived vector.
    pub fn from_scan(
        f: impl FnMut(&mut VecRefMut<T>, &mut SignalContext<'_, '_>) + 'static,
    ) -> Self {
        let mut f = f;
        Self(RawSignalVec::ChangeFeed(ChangeFeedSignal::from_scan(
            VecModel::new(),
            move |edit, sc| {
                let mut items = VecRefMut {
                    data: VecRefMutData(edit),
                };
                f(&mut items, sc);
            },
        )))
    }

    /// Creates an independently advancing change reader.
    pub fn reader(&self) -> SignalVecReader<T> {
        SignalVecReader(match &self.0 {
            RawSignalVec::ChangeFeed(signal) => RawSignalVecReader::ChangeFeed(signal.reader()),
            RawSignalVec::RcVec(vec) => RawSignalVecReader::RcVec {
                vec: vec.clone(),
                has_read: false,
            },
            RawSignalVec::ArcVec(vec) => RawSignalVecReader::ArcVec {
                vec: vec.clone(),
                has_read: false,
            },
            RawSignalVec::RcSlice(slice) => RawSignalVecReader::RcSlice {
                slice: slice.clone(),
                has_read: false,
            },
            RawSignalVec::ArcSlice(slice) => RawSignalVecReader::ArcSlice {
                slice: slice.clone(),
                has_read: false,
            },
            RawSignalVec::StaticSlice(slice) => RawSignalVecReader::StaticSlice {
                slice,
                has_read: false,
            },
        })
    }

    /// Borrows all current items and registers a collection-wide dependency.
    pub fn borrow<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> VecRef<'a, T> {
        match &self.0 {
            RawSignalVec::ChangeFeed(signal) => VecRef::from_ref(signal.borrow(sc)),
            RawSignalVec::RcVec(vec) => VecRef::from_slice_items(vec),
            RawSignalVec::ArcVec(vec) => VecRef::from_slice_items(vec),
            RawSignalVec::RcSlice(slice) => VecRef::from_slice_items(slice),
            RawSignalVec::ArcSlice(slice) => VecRef::from_slice_items(slice),
            RawSignalVec::StaticSlice(slice) => VecRef::from_slice_items(slice),
        }
    }
}
impl<T> From<Vec<T>> for SignalVec<T> {
    fn from(value: Vec<T>) -> Self {
        Rc::new(value).into()
    }
}
impl<T> From<Rc<Vec<T>>> for SignalVec<T> {
    fn from(value: Rc<Vec<T>>) -> Self {
        Self(RawSignalVec::RcVec(value))
    }
}
impl<T> From<&Rc<Vec<T>>> for SignalVec<T> {
    fn from(value: &Rc<Vec<T>>) -> Self {
        Rc::clone(value).into()
    }
}
impl<T> From<Arc<Vec<T>>> for SignalVec<T> {
    fn from(value: Arc<Vec<T>>) -> Self {
        Self(RawSignalVec::ArcVec(value))
    }
}
impl<T> From<&Arc<Vec<T>>> for SignalVec<T> {
    fn from(value: &Arc<Vec<T>>) -> Self {
        Arc::clone(value).into()
    }
}
impl<const N: usize, T> From<[T; N]> for SignalVec<T> {
    fn from(value: [T; N]) -> Self {
        Rc::<[T]>::from(value).into()
    }
}
impl<T> From<Rc<[T]>> for SignalVec<T> {
    fn from(value: Rc<[T]>) -> Self {
        Self(RawSignalVec::RcSlice(value))
    }
}
impl<T> From<&Rc<[T]>> for SignalVec<T> {
    fn from(value: &Rc<[T]>) -> Self {
        Rc::clone(value).into()
    }
}
impl<T> From<Arc<[T]>> for SignalVec<T> {
    fn from(value: Arc<[T]>) -> Self {
        Self(RawSignalVec::ArcSlice(value))
    }
}
impl<T> From<&Arc<[T]>> for SignalVec<T> {
    fn from(value: &Arc<[T]>) -> Self {
        Arc::clone(value).into()
    }
}

impl<T> From<&'static [T]> for SignalVec<T> {
    fn from(value: &'static [T]) -> Self {
        Self(RawSignalVec::StaticSlice(value))
    }
}
impl<const N: usize, T> From<&'static [T; N]> for SignalVec<T> {
    fn from(value: &'static [T; N]) -> Self {
        Self(RawSignalVec::StaticSlice(value))
    }
}

#[derive_ex(Clone)]
enum RawSignalVec<T: 'static> {
    ChangeFeed(ChangeFeedSignal<VecModel<T>>),
    RcVec(Rc<Vec<T>>),
    ArcVec(Arc<Vec<T>>),
    RcSlice(Rc<[T]>),
    ArcSlice(Arc<[T]>),
    StaticSlice(&'static [T]),
}

/// Reads current values and deltas from a signal vector.
///
/// A clone starts at the same cursor position. Subsequent [`read`](Self::read) calls advance each
/// reader independently.
#[derive(Ex)]
#[derive_ex(Clone(bound()))]
pub struct SignalVecReader<T: 'static>(RawSignalVecReader<T>);

#[derive(Ex)]
#[derive_ex(Clone(bound()))]
enum RawSignalVecReader<T: 'static> {
    ChangeFeed(ChangeFeedReader<VecModel<T>>),
    RcVec { vec: Rc<Vec<T>>, has_read: bool },
    ArcVec { vec: Arc<Vec<T>>, has_read: bool },
    RcSlice { slice: Rc<[T]>, has_read: bool },
    ArcSlice { slice: Arc<[T]>, has_read: bool },
    StaticSlice { slice: &'static [T], has_read: bool },
}

impl<T: 'static> SignalVecReader<T> {
    pub fn read<'a, 'r: 'a>(&'a mut self, sc: &mut SignalContext<'r, '_>) -> VecRef<'a, T> {
        match &mut self.0 {
            RawSignalVecReader::ChangeFeed(reader) => VecRef::from_ref(reader.read(sc)),
            RawSignalVecReader::RcVec { vec, has_read } => {
                let items = VecRef::from_slice(vec, !*has_read);
                *has_read = true;
                items
            }
            RawSignalVecReader::ArcVec { vec, has_read } => {
                let items = VecRef::from_slice(vec, !*has_read);
                *has_read = true;
                items
            }
            RawSignalVecReader::RcSlice { slice, has_read } => {
                let items = VecRef::from_slice(slice, !*has_read);
                *has_read = true;
                items
            }
            RawSignalVecReader::ArcSlice { slice, has_read } => {
                let items = VecRef::from_slice(slice, !*has_read);
                *has_read = true;
                items
            }
            RawSignalVecReader::StaticSlice { slice, has_read } => {
                let items = VecRef::from_slice(slice, !*has_read);
                *has_read = true;
                items
            }
        }
    }

    /// Returns the current items and delta since the last [`read`](Self::read) without advancing the reader.
    ///
    /// Before the first `read`, the delta contains an insertion for every current item.
    ///
    /// This method registers the same signal dependency as [`read`](Self::read). Because a subsequent
    /// `peek` or `read` reports the same delta again, callers must not treat the returned delta as
    /// consumed or use it to update retained mirror or element state. Use [`read`](Self::read) when
    /// applying changes to such state.
    pub fn peek<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> VecRef<'a, T> {
        match &self.0 {
            RawSignalVecReader::ChangeFeed(reader) => VecRef::from_ref(reader.peek(sc)),
            RawSignalVecReader::RcVec { vec, has_read } => VecRef::from_slice(vec, !*has_read),
            RawSignalVecReader::ArcVec { vec, has_read } => VecRef::from_slice(vec, !*has_read),
            RawSignalVecReader::RcSlice { slice, has_read } => {
                VecRef::from_slice(slice, !*has_read)
            }
            RawSignalVecReader::ArcSlice { slice, has_read } => {
                VecRef::from_slice(slice, !*has_read)
            }
            RawSignalVecReader::StaticSlice { slice, has_read } => {
                VecRef::from_slice(slice, !*has_read)
            }
        }
    }
}

/// A borrowed reference to the current vector and its reader delta.
pub struct VecRef<'a, T: 'static> {
    items: RawItems<'a, T>,
    immutable_initial: bool,
}

impl<'a, T: 'static> VecRef<'a, T> {
    fn from_slice_items(slice: &'a [T]) -> Self {
        Self::from_slice(slice, false)
    }
    fn from_slice(slice: &'a [T], initial: bool) -> Self {
        Self {
            items: RawItems::Slice(slice),
            immutable_initial: initial,
        }
    }

    fn from_ref(value: ChangeFeedRef<'a, VecModel<T>>) -> Self {
        Self {
            items: RawItems::ChangeFeed(value),
            immutable_initial: false,
        }
    }

    pub fn len(&self) -> usize {
        match &self.items {
            RawItems::ChangeFeed(value) => value.current().items.len(),
            RawItems::Slice(slice) => slice.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, index: usize) -> Option<&T> {
        self.items.get(index)
    }
    /// Returns the delta needed to reproduce the current items from the reader's baseline.
    ///
    /// An initial read returns an [`Insert`](VecChange::Insert) for every current item.
    pub fn delta(&self) -> impl Iterator<Item = VecChange<'_, T>> + '_ {
        use iter_n::iter3::*;
        match &self.items {
            RawItems::ChangeFeed(value) => match value.delta() {
                ChangeFeedDelta::Initial => self.initial_changes().into_iter0(),
                ChangeFeedDelta::Incremental(changes) => changes
                    .map(|change| change.to_signal_vec_change(&value.current().values))
                    .into_iter1(),
            },
            RawItems::Slice(_) if self.immutable_initial => self.initial_changes().into_iter0(),
            RawItems::Slice(_) => [].into_iter2(),
        }
    }

    fn initial_changes(&self) -> impl Iterator<Item = VecChange<'_, T>> + '_ {
        self.iter()
            .enumerate()
            .map(|(index, new_value)| VecChange::Insert { index, new_value })
    }

    pub fn iter(&self) -> Iter<'_, T> {
        Iter::new(match &self.items {
            RawItems::ChangeFeed(value) => IterSource::Model(value.current()),
            RawItems::Slice(slice) => IterSource::Slice(slice),
        })
    }
}

impl<T: 'static> Index<usize> for VecRef<'_, T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("index out of bounds")
    }
}
impl<'a, T: 'static> IntoIterator for &'a VecRef<'_, T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<T: Debug + 'static> Debug for VecRef<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}
impl<T: PartialEq<T>> PartialEq for VecRef<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        if self.len() != other.len() {
            return false;
        }
        for (a, b) in self.iter().zip(other.iter()) {
            if a != b {
                return false;
            }
        }
        true
    }
}
impl<T: PartialEq<T>> PartialEq<[T]> for VecRef<'_, T> {
    fn eq(&self, other: &[T]) -> bool {
        if self.len() != other.len() {
            return false;
        }
        for (a, b) in self.iter().zip(other.iter()) {
            if a != b {
                return false;
            }
        }
        true
    }
}
impl<const N: usize, T: PartialEq<T>> PartialEq<[T; N]> for VecRef<'_, T> {
    fn eq(&self, other: &[T; N]) -> bool {
        self.eq(other.as_slice())
    }
}
impl<T: PartialEq<T>> PartialEq<Vec<T>> for VecRef<'_, T> {
    fn eq(&self, other: &Vec<T>) -> bool {
        self.eq(other.as_slice())
    }
}

enum RawItems<'a, T: 'static> {
    ChangeFeed(ChangeFeedRef<'a, VecModel<T>>),
    Slice(&'a [T]),
}
impl<T: 'static> RawItems<'_, T> {
    fn get(&self, index: usize) -> Option<&T> {
        match self {
            RawItems::ChangeFeed(value) => value.current().get(index),
            RawItems::Slice(slice) => slice.get(index),
        }
    }
}

/// A mutable borrowed reference that records vector changes.
///
/// This guard exposes only operations that record their changes and deliberately does not
/// implement [`DerefMut`].
pub struct VecRefMut<'a, T: 'static> {
    data: VecRefMutData<'a, T>,
}

impl<T: 'static> VecRefMut<'_, T> {
    pub fn len(&self) -> usize {
        self.data.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn capacity(&self) -> usize {
        self.data.items.capacity()
    }
    pub fn reserve(&mut self, additional: usize) {
        self.data.reserve(additional);
    }
    pub fn insert(&mut self, index: usize, value: T) {
        let new_value = self.data.insert_raw(index, value);
        self.data.record(ChangeData::Insert { index, new_value });
    }
    pub fn push(&mut self, value: T) {
        let len = self.len();
        self.insert(len, value);
    }
    pub fn remove(&mut self, index: usize) {
        let old_value = self.data.items.remove(index);
        self.data.record(ChangeData::Remove { index, old_value });
    }
    pub fn get(&self, index: usize) -> Option<&T> {
        self.data.get(index)
    }
    pub fn set(&mut self, index: usize, value: T) {
        let old_value = self.data.items[index];
        let new_value = self.data.values.insert(value);
        self.data.items[index] = new_value;
        self.data.record(ChangeData::Set {
            index,
            old_value,
            new_value,
        });
    }
    pub fn swap(&mut self, index0: usize, index1: usize) {
        if index0 == index1 {
            return;
        }
        self.data.items.swap(index0, index1);
        self.data.record(ChangeData::Swap {
            index: (index0, index1),
        });
    }
    pub fn swap_remove(&mut self, index: usize) {
        assert!(index < self.len(), "index out of bounds");
        let last = self.len() - 1;
        self.swap(index, last);
        self.remove(last);
    }
    pub fn move_item(&mut self, old_index: usize, new_index: usize) {
        match old_index.cmp(&new_index) {
            Ordering::Less => self.data.items[old_index..=new_index].rotate_left(1),
            Ordering::Greater => self.data.items[new_index..=old_index].rotate_right(1),
            Ordering::Equal => return,
        }
        self.data.record(ChangeData::Move {
            old_index,
            new_index,
        });
    }
    pub fn sort(&mut self)
    where
        T: Ord,
    {
        self.sort_by(|a, b| a.cmp(b))
    }
    pub fn sort_by(&mut self, mut compare: impl FnMut(&T, &T) -> Ordering) {
        let order = IndexNewToOld::from_sort_by(&self.data.items, |&a, &b| {
            compare(&self.data.values[a], &self.data.values[b])
        });
        self.reorder_owned(order);
    }
    pub fn sort_by_key<K: Ord>(&mut self, mut key: impl FnMut(&T) -> K) {
        self.sort_by(|a, b| key(a).cmp(&key(b)))
    }

    pub fn sort_unstable(&mut self)
    where
        T: Ord,
    {
        self.sort_unstable_by(|a, b| a.cmp(b))
    }
    pub fn sort_unstable_by(&mut self, mut compare: impl FnMut(&T, &T) -> Ordering) {
        let order = IndexNewToOld::from_sort_unstable_by(&self.data.items, |&a, &b| {
            compare(&self.data.values[a], &self.data.values[b])
        });
        self.reorder_owned(order);
    }
    pub fn sort_unstable_by_key<K: Ord>(&mut self, mut key: impl FnMut(&T) -> K) {
        self.sort_unstable_by(|a, b| key(a).cmp(&key(b)))
    }

    /// Reorders the current items according to `order` and records a [`VecChange::Reorder`].
    ///
    /// Indices refer to the positions immediately before this call. An identity permutation
    /// leaves the items unchanged and records no change.
    ///
    /// # Examples
    ///
    /// ```
    /// use sigmut::{collections::vec::StateVec, core::Runtime, utils::IndexNewToOld};
    ///
    /// let mut rt = Runtime::new();
    /// let values: StateVec<_> = ["a", "b", "c"].into_iter().collect();
    /// let order = IndexNewToOld::from_sort(&[30, 10, 20]);
    /// values.borrow_mut(rt.ac()).reorder(&order);
    /// assert_eq!(values.borrow(&mut rt.sc()), ["b", "c", "a"]);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics before changing any items if the lengths differ, an index is out of bounds,
    /// or an index occurs more than once.
    pub fn reorder(&mut self, order: &IndexNewToOld) {
        self.reorder_owned(order.to_owned());
    }

    fn reorder_owned(&mut self, order: IndexNewToOldBuf) {
        if order.as_slice().iter().copied().eq(0..self.len()) {
            return;
        }
        order.apply_to(&mut self.data.items);
        self.data.record(ChangeData::Reorder { new_to_old: order });
    }

    pub fn drain(&mut self, range: impl RangeBounds<usize>) {
        let range = to_range(range, self.len());
        for index in (range.start..range.end).rev() {
            let old_value = self.data.items[index];
            self.data.record(ChangeData::Remove { index, old_value });
        }
        self.data.items.drain(range);
    }

    pub fn clear(&mut self) {
        self.drain(..);
    }

    pub fn iter(&self) -> Iter<'_, T> {
        Iter::new(IterSource::Model(&self.data))
    }
}

impl<T> Index<usize> for VecRefMut<'_, T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("index out of bounds")
    }
}
impl<'a, T> IntoIterator for &'a VecRefMut<'_, T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<T> Extend<T> for VecRefMut<'_, T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for value in iter {
            self.push(value)
        }
    }
}

impl<T: Debug> Debug for VecRefMut<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}
impl<T: PartialEq<T>> PartialEq for VecRefMut<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        if self.len() != other.len() {
            return false;
        }
        for (a, b) in self.iter().zip(other.iter()) {
            if a != b {
                return false;
            }
        }
        true
    }
}
impl<T: PartialEq<T>> PartialEq<[T]> for VecRefMut<'_, T> {
    fn eq(&self, other: &[T]) -> bool {
        if self.len() != other.len() {
            return false;
        }
        for (a, b) in self.iter().zip(other.iter()) {
            if a != b {
                return false;
            }
        }
        true
    }
}
impl<const N: usize, T: PartialEq<T>> PartialEq<[T; N]> for VecRefMut<'_, T> {
    fn eq(&self, other: &[T; N]) -> bool {
        self.eq(other.as_slice())
    }
}
impl<T: PartialEq<T>> PartialEq<Vec<T>> for VecRefMut<'_, T> {
    fn eq(&self, other: &Vec<T>) -> bool {
        self.eq(other.as_slice())
    }
}

struct VecRefMutData<'a, T: 'static>(ChangeFeedRefMut<'a, VecModel<T>>);

impl<T> VecRefMutData<'_, T> {
    fn record(&mut self, change: ChangeData) {
        self.0.record(change);
    }
}
impl<T> Deref for VecRefMutData<'_, T> {
    type Target = VecModel<T>;

    fn deref(&self) -> &Self::Target {
        self.0.current()
    }
}
impl<T> DerefMut for VecRefMutData<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.current_mut()
    }
}

#[derive(Ex)]
#[derive_ex(Clone(bound()))]
pub struct Iter<'a, T: 'static> {
    items: IterSource<'a, T>,
    index: usize,
}

impl<'a, T> Iter<'a, T> {
    fn new(items: IterSource<'a, T>) -> Self {
        Self { items, index: 0 }
    }
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let value = self.items.get(self.index)?;
        self.index += 1;
        Some(value)
    }
}

#[derive_ex(Clone)]
enum IterSource<'a, T: 'static> {
    Model(&'a VecModel<T>),
    Slice(&'a [T]),
}
impl<'a, T: 'static> IterSource<'a, T> {
    fn get(&self, index: usize) -> Option<&'a T> {
        match self {
            IterSource::Model(data) => data.get(index),
            IterSource::Slice(slice) => slice.get(index),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
#[derive_ex(Clone, Copy, bound())]
pub enum VecChange<'a, T: ?Sized> {
    Insert {
        index: usize,
        new_value: &'a T,
    },
    Remove {
        index: usize,
        old_value: &'a T,
    },
    Set {
        index: usize,
        new_value: &'a T,
        old_value: &'a T,
    },
    Move {
        old_index: usize,
        new_index: usize,
    },
    Swap {
        index: (usize, usize),
    },
    /// Reorders the items present immediately before this change.
    Reorder(&'a IndexNewToOld),
}

#[derive(Debug)]
enum ChangeData {
    Insert {
        index: usize,
        new_value: usize,
    },
    Remove {
        index: usize,
        old_value: usize,
    },
    Set {
        index: usize,
        old_value: usize,
        new_value: usize,
    },
    Move {
        old_index: usize,
        new_index: usize,
    },
    Swap {
        index: (usize, usize),
    },
    Reorder {
        new_to_old: IndexNewToOldBuf,
    },
}

impl ChangeData {
    fn to_signal_vec_change<'a, T>(&'a self, values: &'a SlabMap<T>) -> VecChange<'a, T> {
        match self {
            &ChangeData::Insert { index, new_value } => VecChange::Insert {
                index,
                new_value: &values[new_value],
            },
            &ChangeData::Remove { index, old_value } => VecChange::Remove {
                index,
                old_value: &values[old_value],
            },
            &ChangeData::Set {
                index,
                old_value,
                new_value,
            } => VecChange::Set {
                index,
                old_value: &values[old_value],
                new_value: &values[new_value],
            },
            &ChangeData::Move {
                old_index,
                new_index,
            } => VecChange::Move {
                old_index,
                new_index,
            },
            &ChangeData::Swap { index } => VecChange::Swap { index },
            ChangeData::Reorder { new_to_old } => VecChange::Reorder(new_to_old),
        }
    }
}

#[derive(Ex)]
#[derive_ex(Clone(bound()), Default)]
#[default(Self::new())]
/// Mutable reactive vector state.
pub struct StateVec<T: 'static>(ChangeFeedState<VecModel<T>>);

impl<T> StateVec<T> {
    /// Creates an empty vector.
    pub fn new() -> Self {
        Self(ChangeFeedState::new(VecModel::new()))
    }
    /// Returns a signal backed by this state.
    pub fn to_signal(&self) -> SignalVec<T> {
        SignalVec(RawSignalVec::ChangeFeed(self.0.to_signal()))
    }
    /// Creates an independently advancing change reader.
    pub fn reader(&self) -> SignalVecReader<T> {
        SignalVecReader(RawSignalVecReader::ChangeFeed(self.0.reader()))
    }
    /// Borrows all current items and registers a collection-wide dependency.
    pub fn borrow<'a, 'r: 'a>(&'a self, sc: &mut SignalContext<'r, '_>) -> VecRef<'a, T> {
        VecRef::from_ref(self.0.borrow(sc))
    }
    /// Mutably borrows the vector for a change-recording edit.
    pub fn borrow_mut<'a>(&'a self, ac: &'a mut ActionContext) -> VecRefMut<'a, T> {
        VecRefMut {
            data: VecRefMutData(self.0.borrow_mut(ac)),
        }
    }
    /// Mutably borrows the vector without tying the returned guard to the action context.
    pub fn borrow_mut_loose(&self, ac: &mut ActionContext) -> VecRefMut<'_, T> {
        VecRefMut {
            data: VecRefMutData(self.0.borrow_mut_loose(ac)),
        }
    }
}
impl<T: Serialize> Serialize for StateVec<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let model = self
            .0
            .try_borrow_contextless()
            .map_err(serde::ser::Error::custom)?;
        serializer.collect_seq(model.current().iter())
    }
}
impl<'de, T: Deserialize<'de> + 'static> Deserialize<'de> for StateVec<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct StateVecVisitor<T>(PhantomData<fn(T)>);
        impl<'de, T: Deserialize<'de> + 'static> serde::de::Visitor<'de> for StateVecVisitor<T> {
            type Value = StateVec<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("sequence")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut data = VecModel::new();
                while let Some(value) = seq.next_element()? {
                    data.push_raw(value)
                }
                Ok(StateVec(ChangeFeedState::new(data)))
            }
        }
        deserializer.deserialize_seq(StateVecVisitor(PhantomData))
    }
}
impl<A> FromIterator<A> for StateVec<A> {
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let mut data = VecModel::new();
        let iter = iter.into_iter();
        data.reserve(iter.size_hint().0);
        for i in iter {
            data.push_raw(i);
        }
        Self(ChangeFeedState::new(data))
    }
}

struct VecModel<T> {
    items: Vec<usize>,
    values: SlabMap<T>,
}
impl<T: 'static> VecModel<T> {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            values: SlabMap::new(),
        }
    }
    fn len(&self) -> usize {
        self.items.len()
    }
    fn reserve(&mut self, additional: usize) {
        self.items.reserve(additional);
        self.values.reserve(additional);
    }
    fn insert_raw(&mut self, index: usize, value: T) -> usize {
        let key = self.values.insert(value);
        self.items.insert(index, key);
        key
    }
    fn push_raw(&mut self, value: T) {
        let index = self.len();
        self.insert_raw(index, value);
    }
    fn get(&self, index: usize) -> Option<&T> {
        Some(&self.values[*self.items.get(index)?])
    }
    fn iter(&self) -> Iter<'_, T> {
        Iter::new(IterSource::Model(self))
    }
}

impl<T: 'static> ChangeFeedModel for VecModel<T> {
    type Change = ChangeData;

    fn release_change(&mut self, change: Self::Change) {
        match change {
            ChangeData::Remove { old_value, .. } | ChangeData::Set { old_value, .. } => {
                self.values.remove(old_value);
            }
            ChangeData::Insert { .. }
            | ChangeData::Move { .. }
            | ChangeData::Swap { .. }
            | ChangeData::Reorder { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests;
