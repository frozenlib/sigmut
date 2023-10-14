use std::{
    any::Any,
    cell::{Ref, RefCell, RefMut},
    cmp::Ordering,
    fmt::{self, Debug},
    marker::PhantomData,
    ops::{Deref, DerefMut, Index, IndexMut, RangeBounds},
    rc::Rc,
};

use derive_ex::derive_ex;
use serde::{Deserialize, Serialize};
use slabmap::SlabMap;

use crate::{
    core::{
        schedule_notify, BindSink, BindSource, Computed, SinkBindings, SourceBindings,
        UpdateContext,
    },
    utils::{is_sorted, to_range, Changes, IndexNewToOld, RefCountOps},
    ActionContext, ObsContext,
};

const SLOT_NOT_USED: usize = 0;

#[derive_ex(Clone(bound()))]
pub struct ObsVec<T: 'static>(RawObsVec<T>);

impl<T: 'static> ObsVec<T> {
    pub fn from_scan(f: impl FnMut(&mut ObsVecItemsMut<T>, &mut ObsContext) + 'static) -> Self {
        Self(RawObsVec::Rc(Rc::new(Scan::new(f))))
    }
    pub fn session(&self) -> ObsVecSession<T> {
        ObsVecSession {
            source: self.0.clone(),
            age: None,
        }
    }

    pub fn items(&self, oc: &mut ObsContext) -> ObsVecItems<T> {
        match &self.0 {
            RawObsVec::Rc(rc) => rc.items(rc.clone().into_any(), oc),
            RawObsVec::Slice(slice) => ObsVecItems::from_slice_items(slice),
        }
    }
}
impl<T> From<Vec<T>> for ObsVec<T> {
    fn from(value: Vec<T>) -> Self {
        Rc::new(value).into()
    }
}
impl<T> From<Rc<Vec<T>>> for ObsVec<T> {
    fn from(value: Rc<Vec<T>>) -> Self {
        Self(RawObsVec::Rc(value))
    }
}
impl<T: 'static> ObservableVec<T> for Vec<T> {
    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
    fn items(&self, _this: Rc<dyn Any>, _oc: &mut ObsContext) -> ObsVecItems<T> {
        ObsVecItems::from_slice_items(self)
    }
    fn read_session(
        &self,
        _this: Rc<dyn Any>,
        age: &mut Option<usize>,
        _oc: &mut ObsContext,
    ) -> ObsVecItems<T> {
        ObsVecItems::from_slice_read_session(self, age)
    }

    fn drop_session(&self, _age: usize) {}
}

impl<T> From<&'static [T]> for ObsVec<T> {
    fn from(value: &'static [T]) -> Self {
        Self(RawObsVec::Slice(value))
    }
}

trait ObservableVec<T> {
    fn into_any(self: Rc<Self>) -> Rc<dyn Any>;
    fn items(&self, this: Rc<dyn Any>, oc: &mut ObsContext) -> ObsVecItems<T>;
    fn read_session(
        &self,
        this: Rc<dyn Any>,
        age: &mut Option<usize>,
        oc: &mut ObsContext,
    ) -> ObsVecItems<T>;
    fn drop_session(&self, age: usize);
}

#[derive_ex(Clone)]
enum RawObsVec<T: 'static> {
    Rc(Rc<dyn ObservableVec<T>>),
    Slice(&'static [T]),
}

pub struct ObsVecSession<T: 'static> {
    source: RawObsVec<T>,
    age: Option<usize>,
}

impl<T: 'static> ObsVecSession<T> {
    pub fn read(&mut self, oc: &mut ObsContext) -> ObsVecItems<T> {
        match &self.source {
            RawObsVec::Rc(vec) => vec.read_session(vec.clone().into_any(), &mut self.age, oc),
            RawObsVec::Slice(slice) => ObsVecItems::from_slice_read_session(slice, &mut self.age),
        }
    }
}
impl<T> Drop for ObsVecSession<T> {
    fn drop(&mut self) {
        if let Some(age) = self.age {
            match &self.source {
                RawObsVec::Rc(vec) => vec.drop_session(age),
                RawObsVec::Slice(_) => {}
            }
        }
    }
}

pub struct ObsVecItems<'a, T: 'static> {
    items: RawObsVecItems<'a, T>,
    age_since: Option<usize>,
}

impl<'a, T: 'static> ObsVecItems<'a, T> {
    fn from_slice_read_session(slice: &'a [T], age: &mut Option<usize>) -> Self {
        let age_since = *age;
        *age = Some(0);
        Self::from_slice(slice, age_since)
    }
    fn from_slice_items(slice: &'a [T]) -> Self {
        Self::from_slice(slice, Some(0))
    }
    fn from_slice(slice: &'a [T], age_since: Option<usize>) -> Self {
        Self {
            items: RawObsVecItems::Slice(slice),
            age_since,
        }
    }

    fn from_data_read_session(data: Ref<'a, ItemsData<T>>, age: &mut Option<usize>) -> Self {
        let age_since = *age;
        *age = Some(data.changes.end_age());
        Self::from_data(data, age_since)
    }

    fn from_data_items(data: Ref<'a, ItemsData<T>>) -> Self {
        let age_since = Some(data.changes.end_age());
        Self::from_data(data, age_since)
    }
    fn from_data(data: Ref<'a, ItemsData<T>>, age_since: Option<usize>) -> Self {
        Self {
            items: RawObsVecItems::Cell(data),
            age_since,
        }
    }

    pub fn len(&self) -> usize {
        match &self.items {
            RawObsVecItems::Cell(data) => data.items.len(),
            RawObsVecItems::Slice(slice) => slice.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, index: usize) -> Option<&T> {
        self.items.get(index)
    }
    pub fn changes(&self, mut f: impl FnMut(ObsVecChange<T>)) {
        if let Some(age) = self.age_since {
            match &self.items {
                RawObsVecItems::Cell(data) => data.changes(age, f),
                RawObsVecItems::Slice(_) => {}
            }
        } else {
            for (index, new_value) in self.iter().enumerate() {
                f(ObsVecChange::Insert { index, new_value });
            }
        }
    }
    pub fn iter(&self) -> ObsVecIter<T> {
        ObsVecIter::new(match &self.items {
            RawObsVecItems::Cell(data) => IterSource::Cell(data),
            RawObsVecItems::Slice(slice) => IterSource::Slice(slice),
        })
    }
}

impl<T: 'static> Index<usize> for ObsVecItems<'_, T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("index out of bounds")
    }
}
impl<'a, T: 'static> IntoIterator for &'a ObsVecItems<'_, T> {
    type Item = &'a T;
    type IntoIter = ObsVecIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'a, T: Debug + 'static> Debug for ObsVecItems<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

enum RawObsVecItems<'a, T: 'static> {
    Cell(Ref<'a, ItemsData<T>>),
    Slice(&'a [T]),
}
impl<'a, T: 'static> RawObsVecItems<'a, T> {
    fn get(&self, index: usize) -> Option<&T> {
        match self {
            RawObsVecItems::Cell(data) => data.get(index),
            RawObsVecItems::Slice(slice) => slice.get(index),
        }
    }
}

pub struct ObsVecItemsMut<'a, T: 'static> {
    data: RefMutEx<'a, ItemsData<T>>,
    age: usize,
    cell: Option<&'a ObsVecCell<T>>,
}

impl<'a, T: 'static> ObsVecItemsMut<'a, T> {
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
        self.data
            .changes
            .push(ChangeData::Insert { index, new_value });
    }
    pub fn push(&mut self, value: T) {
        let len = self.len();
        self.insert(len, value);
    }
    pub fn remove(&mut self, index: usize) {
        let old_value = self.data.items.remove(index);
        self.data
            .changes
            .push(ChangeData::Remove { index, old_value });
    }
    pub fn get(&self, index: usize) -> Option<&T> {
        self.data.get(index)
    }
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.data.get_mut(index)
    }
    pub fn set(&mut self, index: usize, value: T) {
        let old_value = self.data.items[index];
        let new_value = self.data.values.insert(value);
        self.data.items[index] = new_value;
        self.data.changes.push(ChangeData::Set {
            index,
            old_value,
            new_value,
        });
    }
    pub fn swap(&mut self, index0: usize, index1: usize) {
        self.data.items.swap(index0, index1);
        self.data.changes.push(ChangeData::Swap {
            index: (index0, index1),
        });
    }
    pub fn move_item(&mut self, old_index: usize, new_index: usize) {
        match old_index.cmp(&new_index) {
            Ordering::Less => self.data.items[old_index..=new_index].rotate_left(1),
            Ordering::Greater => self.data.items[new_index..=old_index].rotate_right(1),
            Ordering::Equal => return,
        }
        self.data.changes.push(ChangeData::Move {
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
    pub fn sort_by(&mut self, compare: impl FnMut(&T, &T) -> Ordering) {
        self.data.sort_as(compare, true)
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
    pub fn sort_unstable_by(&mut self, compare: impl FnMut(&T, &T) -> Ordering) {
        self.data.sort_as(compare, false)
    }
    pub fn sort_unstable_by_key<K: Ord>(&mut self, mut key: impl FnMut(&T) -> K) {
        self.sort_unstable_by(|a, b| key(a).cmp(&key(b)))
    }
    pub fn drain(&mut self, range: impl RangeBounds<usize>) {
        let range = to_range(range, self.len());
        for index in (range.start..range.end).rev() {
            let old_value = self.data.items[index];
            self.data
                .changes
                .push(ChangeData::Remove { index, old_value });
        }
        self.data.items.drain(range);
    }

    pub fn clear(&mut self) {
        self.drain(..);
    }

    pub fn iter(&self) -> ObsVecIter<T> {
        ObsVecIter::new(IterSource::Cell(&self.data))
    }
}
impl<'a, T> Drop for ObsVecItemsMut<'a, T> {
    fn drop(&mut self) {
        if let Some(cell) = self.cell {
            self.data.edit_end_lazy(cell, self.age);
        }
    }
}

impl<T> Index<usize> for ObsVecItemsMut<'_, T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("index out of bounds")
    }
}
impl<T> IndexMut<usize> for ObsVecItemsMut<'_, T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.get_mut(index).expect("index out of bounds")
    }
}
impl<'a, T> IntoIterator for &'a ObsVecItemsMut<'_, T> {
    type Item = &'a T;
    type IntoIter = ObsVecIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T: Debug> Debug for ObsVecItemsMut<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

enum RefMutEx<'a, T> {
    Cell(RefMut<'a, T>),
    Direct(&'a mut T),
}
impl<'a, T> Deref for RefMutEx<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            RefMutEx::Cell(x) => x,
            RefMutEx::Direct(x) => x,
        }
    }
}
impl<'a, T> DerefMut for RefMutEx<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            RefMutEx::Cell(x) => x,
            RefMutEx::Direct(x) => x,
        }
    }
}

#[derive_ex(Clone(bound()))]
pub struct ObsVecIter<'a, T: 'static> {
    items: IterSource<'a, T>,
    index: usize,
}

impl<'a, T> ObsVecIter<'a, T> {
    fn new(items: IterSource<'a, T>) -> Self {
        Self { items, index: 0 }
    }
}

impl<'a, T> Iterator for ObsVecIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let value = self.items.get(self.index)?;
        self.index += 1;
        Some(value)
    }
}

#[derive_ex(Clone)]
enum IterSource<'a, T: 'static> {
    Cell(&'a ItemsData<T>),
    Slice(&'a [T]),
}
impl<'a, T: 'static> IterSource<'a, T> {
    fn get(&self, index: usize) -> Option<&'a T> {
        match self {
            IterSource::Cell(data) => data.get(index),
            IterSource::Slice(slice) => slice.get(index),
        }
    }
}

#[derive(Debug)]
#[derive_ex(Clone, Copy, bound())]
pub enum ObsVecChange<'a, T: ?Sized> {
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
    Sort(&'a IndexNewToOld),
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
    Sort {
        new_to_old: Vec<usize>,
    },
}

impl ChangeData {
    fn to_obs_vec_change<'a, T>(&'a self, values: &'a SlabMap<T>) -> ObsVecChange<'a, T> {
        match self {
            &ChangeData::Insert { index, new_value } => ObsVecChange::Insert {
                index,
                new_value: &values[new_value],
            },
            &ChangeData::Remove { index, old_value } => ObsVecChange::Remove {
                index,
                old_value: &values[old_value],
            },
            &ChangeData::Set {
                index,
                old_value,
                new_value,
            } => ObsVecChange::Set {
                index,
                old_value: &values[old_value],
                new_value: &values[new_value],
            },
            &ChangeData::Move {
                old_index,
                new_index,
            } => ObsVecChange::Move {
                old_index,
                new_index,
            },
            &ChangeData::Swap { index } => ObsVecChange::Swap { index },
            ChangeData::Sort { new_to_old } => ObsVecChange::Sort(IndexNewToOld::new(new_to_old)),
        }
    }
}

#[derive_ex(Clone(bound()), Default)]
#[default(Self::new())]
pub struct ObsVecCell<T: 'static>(Rc<RawObsVecCell<T>>);

impl<T> ObsVecCell<T> {
    pub fn new() -> Self {
        Self(Rc::new(RawObsVecCell::new()))
    }
    pub fn obs_vec(&self) -> ObsVec<T> {
        ObsVec(RawObsVec::Rc(self.0.clone()))
    }
    pub fn session(&self) -> ObsVecSession<T> {
        self.obs_vec().session()
    }

    pub fn borrow_mut(&self, _ac: &mut ActionContext) -> ObsVecItemsMut<T> {
        let mut data = self.0.data.borrow_mut();
        let age = data.edit_start(&self.0.ref_count_ops);
        let data = RefMutEx::Cell(data);
        ObsVecItemsMut {
            data,
            age,
            cell: Some(self),
        }
    }
}
impl<T: Serialize> Serialize for ObsVecCell<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_seq(self.0.data.borrow().iter())
    }
}
impl<'de, T: Deserialize<'de> + 'static> Deserialize<'de> for ObsVecCell<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ObsVecCellVisitor<T>(PhantomData<fn(T)>);
        impl<'de, T: Deserialize<'de> + 'static> serde::de::Visitor<'de> for ObsVecCellVisitor<T> {
            type Value = ObsVecCell<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("sequence")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let cell = ObsVecCell::new();
                let mut data = cell.0.data.borrow_mut();
                while let Some(value) = seq.next_element()? {
                    data.push_raw(value)
                }
                drop(data);
                Ok(cell)
            }
        }
        deserializer.deserialize_seq(ObsVecCellVisitor(PhantomData))
    }
}
impl<A> FromIterator<A> for ObsVecCell<A> {
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let this = Self::new();
        let mut data = this.0.data.borrow_mut();
        let iter = iter.into_iter();
        data.reserve(iter.size_hint().0);
        for i in iter {
            data.push_raw(i);
        }
        drop(data);
        this
    }
}

struct RawObsVecCell<T: 'static> {
    data: RefCell<ItemsData<T>>,
    ref_count_ops: RefCell<RefCountOps>,
    sinks: RefCell<SinkBindings>,
}
impl<T: 'static> RawObsVecCell<T> {
    fn new() -> Self {
        Self {
            data: RefCell::new(ItemsData::new()),
            ref_count_ops: RefCell::new(RefCountOps::new()),
            sinks: RefCell::new(SinkBindings::new()),
        }
    }
    fn watch(self: &Rc<Self>, oc: &mut ObsContext) {
        let this = self.clone();
        self.sinks.borrow_mut().watch(this, SLOT_NOT_USED, oc);
    }
    fn to_this(this: Rc<dyn Any>) -> Rc<Self> {
        this.downcast::<Self>().unwrap()
    }
}
impl<T: 'static> ObservableVec<T> for RawObsVecCell<T> {
    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }

    fn items(&self, this: Rc<dyn Any>, oc: &mut ObsContext) -> ObsVecItems<T> {
        Self::to_this(this).watch(oc);
        ObsVecItems::from_data_items(self.data.borrow())
    }

    fn read_session(
        &self,
        this: Rc<dyn Any>,
        age: &mut Option<usize>,
        oc: &mut ObsContext,
    ) -> ObsVecItems<T> {
        let this = Self::to_this(this);
        this.watch(oc);
        let data = self.data.borrow();
        let mut r = self.ref_count_ops.borrow_mut();
        r.decrement(*age);
        r.increment();
        let age_since = *age;
        *age = Some(data.changes.end_age());
        ObsVecItems {
            items: RawObsVecItems::Cell(self.data.borrow()),
            age_since,
        }
    }

    fn drop_session(&self, age: usize) {
        self.ref_count_ops.borrow_mut().decrement(Some(age))
    }
}

impl<T> BindSource for RawObsVecCell<T> {
    fn flush(self: Rc<Self>, _slot: usize, _uc: &mut UpdateContext) -> bool {
        false
    }
    fn unbind(self: Rc<Self>, _slot: usize, key: usize, _uc: &mut UpdateContext) {
        self.sinks.borrow_mut().unbind(key)
    }
}

struct ItemsData<T> {
    items: Vec<usize>,
    values: SlabMap<T>,
    changes: Changes<ChangeData>,
}
impl<T: 'static> ItemsData<T> {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            values: SlabMap::new(),
            changes: Changes::new(),
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

    fn edit_start(&mut self, ref_count_ops: &RefCell<RefCountOps>) -> usize {
        ref_count_ops.borrow_mut().apply(&mut self.changes);
        self.clean_changes();
        self.changes.end_age()
    }
    fn edit_end(
        &mut self,
        age: usize,
        sinks: &RefCell<SinkBindings>,
        uc: &mut UpdateContext,
    ) -> bool {
        let is_changed = self.changes.end_age() != age;
        if is_changed {
            sinks.borrow_mut().notify(true, uc)
        }
        is_changed
    }
    fn edit_end_lazy(&mut self, cell: &ObsVecCell<T>, age: usize) -> bool {
        let is_changed = self.changes.end_age() != age;
        if is_changed {
            let node = Rc::downgrade(&cell.0);
            schedule_notify(node, SLOT_NOT_USED)
        }
        is_changed
    }
    fn clean_changes(&mut self) {
        self.changes.clean(|d| match d {
            ChangeData::Remove { old_value, .. } | ChangeData::Set { old_value, .. } => {
                self.values.remove(old_value);
            }
            ChangeData::Insert { .. } => {}
            ChangeData::Move { .. } => {}
            ChangeData::Swap { .. } => {}
            ChangeData::Sort { .. } => {}
        });
    }

    fn get(&self, index: usize) -> Option<&T> {
        Some(&self.values[*self.items.get(index)?])
    }
    fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        Some(&mut self.values[*self.items.get(index)?])
    }
    fn changes(&self, age: usize, mut f: impl FnMut(ObsVecChange<T>)) {
        for item in self.changes.items(age) {
            f(item.to_obs_vec_change(&self.values));
        }
    }

    fn sort_as(&mut self, mut compare: impl FnMut(&T, &T) -> Ordering, stable: bool) {
        let mut new_to_old: Vec<_> = (0..self.items.len()).collect();
        let compare = |&i0: &usize, &i1: &usize| {
            compare(&self.values[self.items[i0]], &self.values[self.items[i1]])
        };
        if stable {
            new_to_old.sort_unstable_by(compare);
        } else {
            new_to_old.sort_by(compare);
        }
        if is_sorted(&new_to_old) {
            return;
        }
        IndexNewToOld::new(&new_to_old).apply_to(&mut self.items);
        self.changes.push(ChangeData::Sort { new_to_old });
    }
    fn iter(&self) -> ObsVecIter<T> {
        ObsVecIter::new(IterSource::Cell(self))
    }
}

impl<T: 'static> BindSink for RawObsVecCell<T> {
    fn notify(self: Rc<Self>, _slot: usize, is_modified: bool, uc: &mut UpdateContext) {
        self.sinks.borrow_mut().notify(is_modified, uc)
    }
}

struct Scan<T, F> {
    data: RefCell<ScanData<T, F>>,
    ref_counts: RefCell<RefCountOps>,
    sinks: RefCell<SinkBindings>,
}
impl<T, F> Scan<T, F>
where
    T: 'static,
    F: FnMut(&mut ObsVecItemsMut<T>, &mut ObsContext) + 'static,
{
    fn new(f: F) -> Self {
        Self {
            data: RefCell::new(ScanData::new(f)),
            ref_counts: RefCell::new(RefCountOps::new()),
            sinks: RefCell::new(SinkBindings::new()),
        }
    }
    fn to_this(this: Rc<dyn Any>) -> Rc<Self> {
        this.downcast::<Self>().unwrap()
    }

    fn watch(self: &Rc<Self>, oc: &mut ObsContext) {
        self.update(oc.uc());
        let this = self.clone();
        self.sinks.borrow_mut().watch(this, SLOT_NOT_USED, oc);
    }

    fn update(self: &Rc<Self>, uc: &mut UpdateContext) -> bool {
        if self.data.borrow().computed == Computed::UpToDate {
            return false;
        }
        let this = Rc::downgrade(self);
        let d = &mut *self.data.borrow_mut();
        let age = d.data.edit_start(&self.ref_counts);
        {
            let mut items = ObsVecItemsMut {
                data: RefMutEx::Direct(&mut d.data),
                age,
                cell: None,
            };
            d.bindings
                .compute(this, 0, |cc| (d.f)(&mut items, cc.oc()), uc);
            d.computed = Computed::UpToDate;
        }
        d.data.edit_end(age, &self.sinks, uc)
    }
}
impl<T, F> ObservableVec<T> for Scan<T, F>
where
    T: 'static,
    F: FnMut(&mut ObsVecItemsMut<T>, &mut ObsContext) + 'static,
{
    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }

    fn items(&self, this: Rc<dyn Any>, oc: &mut ObsContext) -> ObsVecItems<T> {
        let this = Self::to_this(this);
        this.watch(oc);
        ObsVecItems::from_data_items(Ref::map(self.data.borrow(), |data| &data.data))
    }

    fn read_session(
        &self,
        this: Rc<dyn Any>,
        age: &mut Option<usize>,
        oc: &mut ObsContext,
    ) -> ObsVecItems<T> {
        let this = Self::to_this(this);
        this.watch(oc);
        ObsVecItems::from_data_read_session(Ref::map(self.data.borrow(), |data| &data.data), age)
    }

    fn drop_session(&self, age: usize) {
        self.ref_counts.borrow_mut().decrement(Some(age))
    }
}
impl<T, F> BindSink for Scan<T, F>
where
    T: 'static,
    F: FnMut(&mut ObsVecItemsMut<T>, &mut ObsContext) + 'static,
{
    fn notify(self: Rc<Self>, _slot: usize, is_modified: bool, uc: &mut UpdateContext) {
        self.sinks.borrow_mut().notify(is_modified, uc)
    }
}
impl<T, F> BindSource for Scan<T, F>
where
    T: 'static,
    F: FnMut(&mut ObsVecItemsMut<T>, &mut ObsContext) + 'static,
{
    fn flush(self: Rc<Self>, _slot: usize, uc: &mut UpdateContext) -> bool {
        if self.data.borrow().computed == Computed::MayBeOutdated {
            self.update(uc)
        } else {
            false
        }
    }

    fn unbind(self: Rc<Self>, _slot: usize, key: usize, _uc: &mut UpdateContext) {
        self.sinks.borrow_mut().unbind(key);
    }
}

struct ScanData<T, F> {
    data: ItemsData<T>,
    bindings: SourceBindings,
    computed: Computed,
    f: F,
}
impl<T, F> ScanData<T, F>
where
    T: 'static,
    F: FnMut(&mut ObsVecItemsMut<T>, &mut ObsContext) + 'static,
{
    fn new(f: F) -> Self {
        Self {
            data: ItemsData::new(),
            bindings: SourceBindings::new(),
            computed: Computed::None,
            f,
        }
    }
}
