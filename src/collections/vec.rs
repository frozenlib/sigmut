use std::{
    any::Any,
    cell::{Ref, RefCell, RefMut},
    cmp::Ordering,
    fmt::{self, Debug},
    marker::PhantomData,
    ops::{Deref, DerefMut, Index, IndexMut, RangeBounds},
    rc::Rc,
};

use derive_ex::{derive_ex, Ex};
use serde::{Deserialize, Serialize};
use slabmap::SlabMap;

use crate::{
    core::{
        schedule_notify, BindKey, BindSink, BindSource, NotifyContext, NotifyLevel, SinkBindings,
        Slot, SourceBinder, UpdateContext,
    },
    utils::{is_sorted, to_range, Changes, IndexNewToOld, RefCountOps},
    ActionContext, SignalContext,
};

#[derive(Ex)]
#[derive_ex(Clone(bound()))]
pub struct SignalVec<T: 'static>(RawSignalVec<T>);

impl<T: 'static> SignalVec<T> {
    pub fn from_scan(f: impl FnMut(&mut ItemsMut<T>, &mut SignalContext) + 'static) -> Self {
        Self(RawSignalVec::Rc(Scan::new(f)))
    }
    pub fn reader(&self) -> SignalVecReader<T> {
        SignalVecReader {
            source: self.0.clone(),
            age: None,
        }
    }

    pub fn borrow<'a, 's: 'a>(&'a self, sc: &mut SignalContext<'s>) -> Items<'a, T> {
        match &self.0 {
            RawSignalVec::Rc(rc) => rc.items(rc.clone().into_any(), sc),
            RawSignalVec::Slice(slice) => Items::from_slice_items(slice),
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
        Self(RawSignalVec::Rc(value))
    }
}
impl<T: 'static> SignalVecNode<T> for Vec<T> {
    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
    fn items(&self, _this: Rc<dyn Any>, _oc: &mut SignalContext) -> Items<'_, T> {
        Items::from_slice_items(self)
    }
    fn read(
        &self,
        _this: Rc<dyn Any>,
        age: &mut Option<usize>,
        _oc: &mut SignalContext,
    ) -> Items<'_, T> {
        Items::from_slice_read(self, age)
    }

    fn drop_reader(&self, _age: usize) {}
}

impl<T> From<&'static [T]> for SignalVec<T> {
    fn from(value: &'static [T]) -> Self {
        Self(RawSignalVec::Slice(value))
    }
}

trait SignalVecNode<T> {
    fn into_any(self: Rc<Self>) -> Rc<dyn Any>;
    fn items(&self, this: Rc<dyn Any>, sc: &mut SignalContext) -> Items<'_, T>;
    fn read(
        &self,
        this: Rc<dyn Any>,
        age: &mut Option<usize>,
        sc: &mut SignalContext,
    ) -> Items<'_, T>;
    fn drop_reader(&self, age: usize);
}

#[derive_ex(Clone)]
enum RawSignalVec<T: 'static> {
    Rc(Rc<dyn SignalVecNode<T>>),
    Slice(&'static [T]),
}

pub struct SignalVecReader<T: 'static> {
    source: RawSignalVec<T>,
    age: Option<usize>,
}

impl<T: 'static> SignalVecReader<T> {
    pub fn read<'a, 's: 'a>(&'a mut self, sc: &mut SignalContext<'s>) -> Items<'a, T> {
        match &self.source {
            RawSignalVec::Rc(vec) => vec.read(vec.clone().into_any(), &mut self.age, sc),
            RawSignalVec::Slice(slice) => Items::from_slice_read(slice, &mut self.age),
        }
    }
}
impl<T> Drop for SignalVecReader<T> {
    fn drop(&mut self) {
        if let Some(age) = self.age {
            match &self.source {
                RawSignalVec::Rc(vec) => vec.drop_reader(age),
                RawSignalVec::Slice(_) => {}
            }
        }
    }
}

pub struct Items<'a, T: 'static> {
    items: RawItems<'a, T>,
    age_since: Option<usize>,
}

impl<'a, T: 'static> Items<'a, T> {
    fn from_slice_read(slice: &'a [T], age: &mut Option<usize>) -> Self {
        let age_since = *age;
        *age = Some(0);
        Self::from_slice(slice, age_since)
    }
    fn from_slice_items(slice: &'a [T]) -> Self {
        Self::from_slice(slice, Some(0))
    }
    fn from_slice(slice: &'a [T], age_since: Option<usize>) -> Self {
        Self {
            items: RawItems::Slice(slice),
            age_since,
        }
    }

    fn from_data_read(data: Ref<'a, ItemsData<T>>, age: &mut Option<usize>) -> Self {
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
            items: RawItems::Cell(data),
            age_since,
        }
    }

    pub fn len(&self) -> usize {
        match &self.items {
            RawItems::Cell(data) => data.items.len(),
            RawItems::Slice(slice) => slice.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, index: usize) -> Option<&T> {
        self.items.get(index)
    }
    pub fn changes(&self) -> impl Iterator<Item = VecChange<'_, T>> + '_ {
        use iter_n::iter3::*;
        if let Some(age) = self.age_since {
            match &self.items {
                RawItems::Cell(data) => data.changes(age).into_iter0(),
                RawItems::Slice(_) => [].into_iter1(),
            }
        } else {
            self.iter()
                .enumerate()
                .map(|(index, new_value)| VecChange::Insert { index, new_value })
                .into_iter2()
        }
    }
    pub fn iter(&self) -> Iter<'_, T> {
        Iter::new(match &self.items {
            RawItems::Cell(data) => IterSource::Cell(data),
            RawItems::Slice(slice) => IterSource::Slice(slice),
        })
    }
}

impl<T: 'static> Index<usize> for Items<'_, T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("index out of bounds")
    }
}
impl<'a, T: 'static> IntoIterator for &'a Items<'_, T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<T: Debug + 'static> Debug for Items<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

enum RawItems<'a, T: 'static> {
    Cell(Ref<'a, ItemsData<T>>),
    Slice(&'a [T]),
}
impl<T: 'static> RawItems<'_, T> {
    fn get(&self, index: usize) -> Option<&T> {
        match self {
            RawItems::Cell(data) => data.get(index),
            RawItems::Slice(slice) => slice.get(index),
        }
    }
}

pub struct ItemsMut<'a, T: 'static> {
    data: ItemsMutData<'a, T>,
    age: usize,
}

impl<T: 'static> ItemsMut<'_, T> {
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
        if index0 == index1 {
            return;
        }
        self.data.items.swap(index0, index1);
        self.data.changes.push(ChangeData::Swap {
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

    pub fn iter(&self) -> Iter<'_, T> {
        Iter::new(IterSource::Cell(&self.data))
    }
    fn is_dirty(&self) -> bool {
        self.data.is_dirty(self.age)
    }
}
impl<T> Drop for ItemsMut<'_, T> {
    fn drop(&mut self) {
        if self.is_dirty() {
            if let ItemsMutData::Cell {
                node: Some(node),
                nc,
                ..
            } = &mut self.data
            {
                node.schedule_notify(nc);
            }
        }
    }
}

impl<T> Index<usize> for ItemsMut<'_, T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("index out of bounds")
    }
}
impl<T> IndexMut<usize> for ItemsMut<'_, T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.get_mut(index).expect("index out of bounds")
    }
}
impl<'a, T> IntoIterator for &'a ItemsMut<'_, T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<T> Extend<T> for ItemsMut<'_, T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for value in iter {
            self.push(value)
        }
    }
}

impl<T: Debug> Debug for ItemsMut<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

enum ItemsMutData<'a, T: 'static> {
    Cell {
        data: RefMut<'a, ItemsData<T>>,
        node: Option<&'a StateVec<T>>,
        nc: Option<&'a mut NotifyContext>,
    },
    Direct(&'a mut ItemsData<T>),
}
impl<T> Deref for ItemsMutData<'_, T> {
    type Target = ItemsData<T>;

    fn deref(&self) -> &Self::Target {
        match self {
            ItemsMutData::Cell { data, .. } => data,
            ItemsMutData::Direct(x) => x,
        }
    }
}
impl<T> DerefMut for ItemsMutData<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            ItemsMutData::Cell { data, .. } => data,
            ItemsMutData::Direct(x) => x,
        }
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
            ChangeData::Sort { new_to_old } => VecChange::Sort(IndexNewToOld::new(new_to_old)),
        }
    }
}

#[derive(Ex)]
#[derive_ex(Clone(bound()), Default)]
#[default(Self::new())]
pub struct StateVec<T: 'static>(Rc<RawStateVec<T>>);

impl<T> StateVec<T> {
    pub fn new() -> Self {
        Self(Rc::new(RawStateVec::new()))
    }
    pub fn to_signal_vec(&self) -> SignalVec<T> {
        SignalVec(RawSignalVec::Rc(self.0.clone()))
    }
    pub fn reader(&self) -> SignalVecReader<T> {
        self.to_signal_vec().reader()
    }
    pub fn borrow<'a, 's: 'a>(&'a self, sc: &mut SignalContext<'s>) -> Items<'a, T> {
        self.0.items(self.0.clone().into_any(), sc)
    }
    pub fn borrow_mut<'a>(&'a self, ac: &'a mut ActionContext) -> ItemsMut<'a, T> {
        let mut data = self.0.data.borrow_mut();
        let age = data.edit_start(&self.0.ref_count_ops);
        let data = ItemsMutData::Cell {
            data,
            node: Some(self),
            nc: Some(ac.nc()),
        };
        ItemsMut { data, age }
    }
    pub fn borrow_mut_loose(&self, _ac: &mut ActionContext) -> ItemsMut<'_, T> {
        let mut data = self.0.data.borrow_mut();
        let age = data.edit_start(&self.0.ref_count_ops);
        let data = ItemsMutData::Cell {
            data,
            node: Some(self),
            nc: None,
        };
        ItemsMut { data, age }
    }
    fn schedule_notify(&self, nc: &mut Option<&mut NotifyContext>) {
        if let Some(nc) = nc {
            self.0.sinks.borrow_mut().notify(NotifyLevel::Dirty, nc)
        } else {
            let node = Rc::downgrade(&self.0);
            schedule_notify(node, Slot(0))
        }
    }
}
impl<T: Serialize> Serialize for StateVec<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_seq(self.0.data.borrow().iter())
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
                let cell = StateVec::new();
                let mut data = cell.0.data.borrow_mut();
                while let Some(value) = seq.next_element()? {
                    data.push_raw(value)
                }
                drop(data);
                Ok(cell)
            }
        }
        deserializer.deserialize_seq(StateVecVisitor(PhantomData))
    }
}
impl<A> FromIterator<A> for StateVec<A> {
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

struct RawStateVec<T: 'static> {
    data: RefCell<ItemsData<T>>,
    ref_count_ops: RefCell<RefCountOps>,
    sinks: RefCell<SinkBindings>,
}
impl<T: 'static> RawStateVec<T> {
    fn new() -> Self {
        Self {
            data: RefCell::new(ItemsData::new()),
            ref_count_ops: RefCell::new(RefCountOps::new()),
            sinks: RefCell::new(SinkBindings::new()),
        }
    }
    fn watch(self: &Rc<Self>, sc: &mut SignalContext) {
        let this = self.clone();
        self.sinks.borrow_mut().bind(this, Slot(0), sc);
    }
    fn to_this(this: Rc<dyn Any>) -> Rc<Self> {
        this.downcast::<Self>().unwrap()
    }
}
impl<T: 'static> SignalVecNode<T> for RawStateVec<T> {
    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }

    fn items(&self, this: Rc<dyn Any>, sc: &mut SignalContext) -> Items<'_, T> {
        Self::to_this(this).watch(sc);
        Items::from_data_items(self.data.borrow())
    }

    fn read(
        &self,
        this: Rc<dyn Any>,
        age: &mut Option<usize>,
        sc: &mut SignalContext,
    ) -> Items<'_, T> {
        let this = Self::to_this(this);
        this.watch(sc);
        let data = self.data.borrow();
        let mut r = self.ref_count_ops.borrow_mut();
        r.decrement(*age);
        r.increment();
        let age_since = *age;
        *age = Some(data.changes.end_age());
        Items {
            items: RawItems::Cell(self.data.borrow()),
            age_since,
        }
    }

    fn drop_reader(&self, age: usize) {
        self.ref_count_ops.borrow_mut().decrement(Some(age))
    }
}

impl<T> BindSource for RawStateVec<T> {
    fn check(self: Rc<Self>, _slot: Slot, _key: BindKey, _uc: &mut UpdateContext) -> bool {
        false
    }
    fn unbind(self: Rc<Self>, _slot: Slot, key: BindKey, uc: &mut UpdateContext) {
        self.sinks.borrow_mut().unbind(key, uc);
    }

    fn rebind(self: Rc<Self>, slot: Slot, key: BindKey, sc: &mut SignalContext) {
        self.sinks.borrow_mut().rebind(self.clone(), slot, key, sc);
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
    fn changes(&self, age: usize) -> impl Iterator<Item = VecChange<'_, T>> + '_ {
        self.changes
            .items(age)
            .map(|x| x.to_signal_vec_change(&self.values))
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
    fn iter(&self) -> Iter<'_, T> {
        Iter::new(IterSource::Cell(self))
    }

    fn is_dirty(&self, age: usize) -> bool {
        self.changes.end_age() != age
    }
}

impl<T: 'static> BindSink for RawStateVec<T> {
    fn notify(self: Rc<Self>, _slot: Slot, level: NotifyLevel, nc: &mut NotifyContext) {
        self.sinks.borrow_mut().notify(level, nc)
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
    F: FnMut(&mut ItemsMut<T>, &mut SignalContext) + 'static,
{
    fn new(f: F) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            data: RefCell::new(ScanData {
                data: ItemsData::new(),
                sb: SourceBinder::new(this, Slot(0)),
                f,
            }),
            ref_counts: RefCell::new(RefCountOps::new()),
            sinks: RefCell::new(SinkBindings::new()),
        })
    }
    fn to_this(this: Rc<dyn Any>) -> Rc<Self> {
        this.downcast::<Self>().unwrap()
    }

    fn watch(self: &Rc<Self>, sc: &mut SignalContext) {
        self.update(sc.uc());
        let this = self.clone();
        self.sinks.borrow_mut().bind(this, Slot(0), sc);
    }

    fn update(self: &Rc<Self>, uc: &mut UpdateContext) {
        if uc.borrow(&self.data).sb.is_clean() {
            return;
        }
        let d = &mut *self.data.borrow_mut();
        let mut is_dirty = false;
        if d.sb.check(uc) {
            let age = d.data.edit_start(&self.ref_counts);
            let mut items = ItemsMut {
                data: ItemsMutData::Direct(&mut d.data),
                age,
            };
            d.sb.update(|sc| (d.f)(&mut items, sc), uc);
            is_dirty = items.is_dirty();
        }
        self.sinks.borrow_mut().update(is_dirty, uc);
    }
}
impl<T, F> SignalVecNode<T> for Scan<T, F>
where
    T: 'static,
    F: FnMut(&mut ItemsMut<T>, &mut SignalContext) + 'static,
{
    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }

    fn items(&self, this: Rc<dyn Any>, sc: &mut SignalContext) -> Items<'_, T> {
        let this = Self::to_this(this);
        this.watch(sc);
        Items::from_data_items(Ref::map(self.data.borrow(), |data| &data.data))
    }

    fn read(
        &self,
        this: Rc<dyn Any>,
        age: &mut Option<usize>,
        sc: &mut SignalContext,
    ) -> Items<'_, T> {
        let this = Self::to_this(this);
        this.watch(sc);
        Items::from_data_read(Ref::map(self.data.borrow(), |data| &data.data), age)
    }

    fn drop_reader(&self, age: usize) {
        self.ref_counts.borrow_mut().decrement(Some(age))
    }
}
impl<T, F> BindSink for Scan<T, F>
where
    T: 'static,
    F: FnMut(&mut ItemsMut<T>, &mut SignalContext) + 'static,
{
    fn notify(self: Rc<Self>, slot: Slot, level: NotifyLevel, nc: &mut NotifyContext) {
        if self.data.borrow_mut().sb.on_notify(slot, level) {
            self.sinks.borrow_mut().notify(level, nc)
        }
    }
}
impl<T, F> BindSource for Scan<T, F>
where
    T: 'static,
    F: FnMut(&mut ItemsMut<T>, &mut SignalContext) + 'static,
{
    fn check(self: Rc<Self>, _slot: Slot, key: BindKey, uc: &mut UpdateContext) -> bool {
        self.update(uc);
        self.sinks.borrow().is_dirty(key, uc)
    }

    fn unbind(self: Rc<Self>, _slot: Slot, key: BindKey, uc: &mut UpdateContext) {
        self.sinks.borrow_mut().unbind(key, uc);
    }

    fn rebind(self: Rc<Self>, slot: Slot, key: BindKey, sc: &mut SignalContext) {
        self.sinks.borrow_mut().rebind(self.clone(), slot, key, sc);
    }
}

struct ScanData<T, F> {
    data: ItemsData<T>,
    sb: SourceBinder,
    f: F,
}
