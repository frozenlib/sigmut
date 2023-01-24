use super::{
    IndexMapping, ObsVec, ObsVecChange, ObsVecIter, ObsVecSession, ObservableVec,
    ObservableVecItems,
};
use crate::{
    core::{
        dependency_node::{Compute, DependencyNode, DependencyNodeSettings},
        BindSink, BindSource, ComputeContext, Runtime, SinkBindings,
    },
    ActionContext,
};
use derive_ex::derive_ex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use slabmap::SlabMap;
use std::{
    cell::{BorrowMutError, RefCell, RefMut},
    collections::VecDeque,
    fmt::Debug,
    mem::take,
    ops::{Bound, Deref, DerefMut, Index, Range, RangeBounds},
    rc::Rc,
};

#[cfg(test)]
mod cell_tests;

const PARAM: usize = 0;

#[derive(Debug)]
struct ChangeEntry {
    ref_count: usize,
    data: ChangeData,
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
    Sort(IndexMapping),
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
            ChangeData::Sort(m) => ObsVecChange::Sort(m),
        }
    }
}

#[derive(Default)]
struct RawRefCountLogs {
    increments: usize,
    decrement_ages: Vec<usize>,
}

#[derive(Default)]
struct RefCountLogs(RefCell<RawRefCountLogs>);

impl RefCountLogs {
    fn increment<T>(&self, items: Result<RefMut<ObsVecItemsMut<T>>, BorrowMutError>) {
        if let Ok(mut items) = items {
            items.increment_ref_count();
        } else {
            self.0.borrow_mut().increments += 1;
        }
    }
    fn decrement<T>(
        &self,
        items: Result<RefMut<ObsVecItemsMut<T>>, BorrowMutError>,
        age: Option<usize>,
    ) {
        if let Ok(mut items) = items {
            self.apply(&mut items);
            if let Some(age) = age {
                items.decrement_ref_count(age);
            }
        } else if let Some(age) = age {
            self.0.borrow_mut().decrement_ages.push(age);
        }
    }
    fn apply<T>(&self, items: &mut ObsVecItemsMut<T>) {
        items.end_ref_count += take(&mut self.0.borrow_mut().increments);
        while let Some(age) = self.0.borrow_mut().decrement_ages.pop() {
            items.decrement_ref_count(age);
        }
    }
}

pub struct ObsVecItemsMut<T> {
    items: Vec<usize>,
    values: SlabMap<T>,
    age_base: usize,
    changes: VecDeque<ChangeEntry>,
    end_ref_count: usize,
    is_modified: bool,
}
impl<T> ObsVecItemsMut<T> {
    fn new(items: impl IntoIterator<Item = T>) -> Self {
        let mut values = SlabMap::new();
        let items = items.into_iter().map(|item| values.insert(item)).collect();
        Self {
            items,
            values,
            age_base: 0,
            changes: VecDeque::new(),
            end_ref_count: 0,
            is_modified: false,
        }
    }
    fn increment_ref_count(&mut self) {
        self.end_ref_count = self.end_ref_count.wrapping_add(1);
    }
    fn decrement_ref_count(&mut self, age: usize) {
        let index = self.age_to_index(age);
        if let Some(change) = self.changes.get_mut(index) {
            change.ref_count -= 1;
            self.try_clean_changes();
        }
    }
    fn end_age(&self) -> usize {
        self.age_base.wrapping_add(self.changes.len())
    }
    fn age_to_index(&self, age: usize) -> usize {
        let index = age.wrapping_sub(self.age_base);
        assert!(index <= self.changes.len());
        index
    }
    fn try_clean_changes(&mut self) {
        while let Some(change) = self.changes.front() {
            if change.ref_count != 0 {
                return;
            }
            if let ChangeData::Remove { old_value, .. } | ChangeData::Set { old_value, .. } =
                change.data
            {
                self.values.remove(old_value);
            }
            self.age_base = self.age_base.wrapping_add(1);
            self.changes.pop_front();
        }
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn insert(&mut self, index: usize, value: T) {
        let value = self.values.insert(value);
        self.items.insert(index, value);
        self.push_change(ChangeData::Insert {
            index,
            new_value: value,
        });
    }
    pub fn push(&mut self, value: T) {
        let len = self.len();
        self.insert(len, value);
    }
    pub fn remove(&mut self, index: usize) {
        let old_value = self.items.remove(index);
        self.push_change(ChangeData::Remove { index, old_value });
    }
    pub fn set(&mut self, index: usize, value: T) {
        let old_value = self.items[index];
        let new_value = self.values.insert(value);
        self.items[index] = new_value;
        self.push_change(ChangeData::Set {
            index,
            old_value,
            new_value,
        });
    }
    pub fn swap(&mut self, index0: usize, index1: usize) {
        self.items.swap(index0, index1);
        self.push_change(ChangeData::Swap {
            index: (index0, index1),
        });
    }
    pub fn move_item(&mut self, old_index: usize, new_index: usize) {
        match old_index.cmp(&new_index) {
            std::cmp::Ordering::Less => self.items[old_index..=new_index].rotate_left(1),
            std::cmp::Ordering::Greater => self.items[new_index..=old_index].rotate_right(1),
            std::cmp::Ordering::Equal => return,
        }
        self.push_change(ChangeData::Move {
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
    pub fn sort_by(&mut self, compare: impl FnMut(&T, &T) -> std::cmp::Ordering) {
        self.sort_as(compare, true)
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
    pub fn sort_unstable_by(&mut self, compare: impl FnMut(&T, &T) -> std::cmp::Ordering) {
        self.sort_as(compare, false)
    }
    pub fn sort_unstable_by_key<K: Ord>(&mut self, mut key: impl FnMut(&T) -> K) {
        self.sort_unstable_by(|a, b| key(a).cmp(&key(b)))
    }

    fn sort_as(&mut self, mut compare: impl FnMut(&T, &T) -> std::cmp::Ordering, stable: bool) {
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
        let m = IndexMapping::new(new_to_old);
        m.apply_to(&mut self.items);
        self.push_change(ChangeData::Sort(m));
    }
    pub fn drain(&mut self, range: impl RangeBounds<usize>) {
        let range = to_range(range, self.len());
        for index in (range.start..range.end).rev() {
            let old_value = self.items[index];
            self.push_change(ChangeData::Remove { index, old_value });
        }
        self.items.drain(range);
    }

    pub fn clear(&mut self) {
        self.drain(..);
    }

    fn push_change(&mut self, data: ChangeData) {
        self.is_modified = true;
        let ref_count = take(&mut self.end_ref_count);
        if ref_count == 0 && self.changes.is_empty() {
            return;
        }
        let e = ChangeEntry { data, ref_count };
        self.changes.push_back(e);
    }

    pub fn iter(&self) -> ObsVecIter<T> {
        ObsVecIter::new(self)
    }
}
impl<T> ObservableVecItems for ObsVecItemsMut<T> {
    type Item = T;

    fn len(&self) -> usize {
        self.items.len()
    }
    fn get(&self, index: usize) -> Option<&Self::Item> {
        Some(&self.values[*self.items.get(index)?])
    }
    fn changes(&self, age: usize, f: &mut dyn FnMut(super::ObsVecChange<Self::Item>)) {
        let start = self.age_to_index(age);
        let end = self.changes.len();
        for index in start..end {
            f(self.changes[index].data.to_obs_vec_change(&self.values));
        }
    }
}
impl<T> Index<usize> for ObsVecItemsMut<T> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        &self.values[self.items[index]]
    }
}
impl<'a, T> IntoIterator for &'a ObsVecItemsMut<T> {
    type Item = &'a T;
    type IntoIter = ObsVecIter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

struct RawObsVecCell<T> {
    items: RefCell<ObsVecItemsMut<T>>,
    ref_count_logs: RefCountLogs,
    sinks: RefCell<SinkBindings>,
}

impl<T: 'static> ObservableVec for RawObsVecCell<T> {
    type Item = T;

    fn increment_ref_count(&self) -> usize {
        self.ref_count_logs.increment(self.items.try_borrow_mut());
        self.items.borrow().end_age()
    }
    fn decrement_ref_count(&self, age: Option<usize>) {
        self.ref_count_logs
            .decrement(self.items.try_borrow_mut(), age)
    }

    fn with(
        self: Rc<Self>,
        f: &mut dyn FnMut(&dyn ObservableVecItems<Item = Self::Item>, &mut crate::ObsContext),
        oc: &mut crate::ObsContext,
    ) {
        let this = self.clone();
        self.sinks.borrow_mut().watch(this, PARAM, oc);
        f(&*self.items.borrow(), oc);
    }
}
impl<T: 'static> BindSource for RawObsVecCell<T> {
    fn flush(self: Rc<Self>, _param: usize, _rt: &mut Runtime) -> bool {
        false
    }
    fn unbind(self: Rc<Self>, _param: usize, key: usize, _rt: &mut Runtime) {
        self.sinks.borrow_mut().unbind(key)
    }
}
impl<T: 'static> BindSink for RawObsVecCell<T> {
    fn notify(self: Rc<Self>, _param: usize, is_modified: bool, rt: &mut Runtime) {
        self.sinks.borrow_mut().notify(is_modified, rt)
    }
}

#[derive_ex(Clone, Default)]
#[default(Self::new())]
pub struct ObsVecCell<T>(Rc<RawObsVecCell<T>>);

impl<T> ObsVecCell<T> {
    pub fn new() -> Self {
        Self::from([])
    }
    pub fn from(items: impl IntoIterator<Item = T>) -> Self {
        Self(Rc::new(RawObsVecCell {
            items: RefCell::new(ObsVecItemsMut::new(items)),
            ref_count_logs: RefCountLogs::default(),
            sinks: RefCell::new(SinkBindings::new()),
        }))
    }
}
impl<T: 'static> ObsVecCell<T> {
    pub fn obs(&self) -> ObsVec<T> {
        ObsVec::from_rc(self.0.clone())
    }
    pub fn session(&self) -> ObsVecSession<T> {
        self.obs().session()
    }

    pub fn borrow_mut(&self, _ac: &mut ActionContext) -> ObsVecCellRefMut<T> {
        let owner = Rc::clone(&self.0);
        let items = self.0.items.borrow_mut();
        ObsVecCellRefMut {
            owner,
            items,
            is_modified: false,
        }
    }
    pub fn debug(&self) -> ObsVecCellDebug<T> {
        ObsVecCellDebug(self)
    }

    #[cfg(test)]
    fn changes_len(&self) -> usize {
        self.0.items.borrow().changes.len()
    }
}
impl<T: Debug> Debug for ObsVecCell<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0.items.try_borrow() {
            Ok(items) => f.debug_list().entries(items.iter()).finish(),
            Err(_) => f.debug_tuple("ObsVecCell").field(&"<borrowed>").finish(),
        }
    }
}

impl<T: Serialize> Serialize for ObsVecCell<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_seq(self.0.items.borrow().iter())
    }
}
impl<'de, T: Deserialize<'de> + 'static> Deserialize<'de> for ObsVecCell<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let items = Vec::<T>::deserialize(deserializer)?;
        Ok(Self::from(items))
    }
}

pub struct ObsVecCellDebug<'a, T>(&'a ObsVecCell<T>);

impl<T: Debug> Debug for ObsVecCellDebug<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl<T: PartialEq> PartialEq for ObsVecCellDebug<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.0
             .0
            .items
            .borrow()
            .iter()
            .eq(other.0 .0.items.borrow().iter())
    }
}

pub struct ObsVecCellRefMut<'a, T: 'static> {
    owner: Rc<RawObsVecCell<T>>,
    items: std::cell::RefMut<'a, ObsVecItemsMut<T>>,
    is_modified: bool,
}
impl<T> Deref for ObsVecCellRefMut<'_, T> {
    type Target = ObsVecItemsMut<T>;
    fn deref(&self) -> &Self::Target {
        &self.items
    }
}
impl<T> DerefMut for ObsVecCellRefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.items
    }
}

impl<'a, T: 'static> Drop for ObsVecCellRefMut<'a, T> {
    fn drop(&mut self) {
        self.owner.ref_count_logs.apply(&mut self.items);
        if self.is_modified {
            self.is_modified = false;
            let node = Rc::downgrade(&self.owner);
            Runtime::schedule_notify_lazy(node, PARAM);
        }
    }
}

#[derive(Default)]
pub(crate) struct RawScanProps {
    ref_count_logs: RefCountLogs,
}
pub(crate) struct RawScan<T, F> {
    items: ObsVecItemsMut<T>,
    f: F,
}
impl<T, F> RawScan<T, F>
where
    T: 'static,
    F: 'static + FnMut(&mut ObsVecItemsMut<T>, &mut crate::ObsContext),
{
    pub fn new(
        initial_state: impl IntoIterator<Item = T>,
        f: F,
    ) -> Rc<DependencyNode<Self, RawScanProps>> {
        DependencyNode::new(
            Self {
                items: ObsVecItemsMut::new(initial_state),
                f,
            },
            DependencyNodeSettings {
                is_flush: false,
                is_hot: false,
                is_modify_always: false,
            },
        )
    }
}

impl<T, F> Compute for RawScan<T, F>
where
    T: 'static,
    F: 'static + FnMut(&mut ObsVecItemsMut<T>, &mut crate::ObsContext),
{
    fn compute(&mut self, cc: &mut ComputeContext) -> bool {
        (self.f)(&mut self.items, cc.oc());
        take(&mut self.items.is_modified)
    }
}

impl<T, F> ObservableVec for DependencyNode<RawScan<T, F>, RawScanProps>
where
    T: 'static,
    F: 'static + FnMut(&mut ObsVecItemsMut<T>, &mut crate::ObsContext),
{
    type Item = T;

    fn increment_ref_count(&self) -> usize {
        self.data
            .ref_count_logs
            .increment(try_borrow_mut_data(self));
        self.borrow().items.end_age()
    }

    fn decrement_ref_count(&self, age: Option<usize>) {
        self.data
            .ref_count_logs
            .decrement(try_borrow_mut_data(self), age)
    }

    fn with(
        self: Rc<Self>,
        f: &mut dyn FnMut(&dyn ObservableVecItems<Item = Self::Item>, &mut crate::ObsContext),
        oc: &mut crate::ObsContext,
    ) {
        self.watch(oc);
        f(&self.borrow().items, oc);
    }
}
fn try_borrow_mut_data<T, F>(
    node: &DependencyNode<RawScan<T, F>, RawScanProps>,
) -> Result<RefMut<ObsVecItemsMut<T>>, BorrowMutError>
where
    T: 'static,
    F: 'static + FnMut(&mut ObsVecItemsMut<T>, &mut crate::ObsContext),
{
    Ok(RefMut::map(node.try_borrow_mut()?, |scan| &mut scan.items))
}

fn to_range(range: impl RangeBounds<usize>, len: usize) -> Range<usize> {
    let start = match range.start_bound() {
        Bound::Included(&n) => n,
        Bound::Excluded(&n) => n + 1,
        Bound::Unbounded => 0,
    };
    let end = match range.end_bound() {
        Bound::Included(&n) => n + 1,
        Bound::Excluded(&n) => n,
        Bound::Unbounded => len,
    };
    assert!(start <= end);
    assert!(end <= len);
    start..end
}
fn is_sorted(items: &[usize]) -> bool {
    for i in 1..items.len() {
        if items[i - 1] > items[i] {
            return false;
        }
    }
    true
}
