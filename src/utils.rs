use std::{
    any::Any,
    borrow::Borrow,
    cmp::Ordering,
    collections::VecDeque,
    mem::transmute,
    ops::{Bound, Deref, Range, RangeBounds},
};

pub mod sync;

#[cfg(test)]
pub mod test_helpers;

pub fn downcast<T: 'static, S: 'static>(value: S) -> Result<T, S> {
    let mut value = Some(value);
    if let Some(value) = <dyn Any>::downcast_mut::<Option<T>>(&mut value) {
        Ok(value.take().unwrap())
    } else {
        Err(value.unwrap())
    }
}

#[test]
fn test_downcast() {
    assert_eq!(downcast::<u32, _>(10u32), Ok(10u32));
    assert_eq!(downcast::<u16, _>(10u32), Err(10u32));
}

pub fn downcast_or<T: Any, S: Any>(value: S, f: impl FnOnce(S) -> T) -> T {
    match downcast(value) {
        Ok(b) => b,
        Err(v) => f(v),
    }
}

#[allow(clippy::redundant_clone)]
pub(crate) fn into_owned<T>(value: T) -> T::Owned
where
    T: ToOwned + 'static,
    T::Owned: 'static,
{
    match downcast::<T::Owned, _>(value) {
        Ok(value) => value,
        Err(value) => value.to_owned(),
    }
}

/// Maps each new position to its old position in a permutation.
#[derive(Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct IndexNewToOld([usize]);
impl IndexNewToOld {
    /// Borrows indices without validating that they form a permutation.
    pub fn new(new_to_old: &[usize]) -> &Self {
        unsafe { transmute(new_to_old) }
    }

    /// Creates the permutation that stably sorts `items` in ascending order.
    ///
    /// The input is unchanged, and the result does not borrow it.
    pub fn from_sort<T: Ord>(items: &[T]) -> IndexNewToOldBuf {
        Self::from_sort_by(items, Ord::cmp)
    }

    /// Creates the permutation that stably sorts `items` by a key.
    ///
    /// The input is unchanged. Equal keys retain their original order, and `key` may be
    /// called more than once per item.
    pub fn from_sort_by_key<T, K: Ord>(
        items: &[T],
        mut key: impl FnMut(&T) -> K,
    ) -> IndexNewToOldBuf {
        Self::from_sort_by(items, |a, b| key(a).cmp(&key(b)))
    }

    /// Creates the permutation that stably sorts `items` with a comparison function.
    ///
    /// The input is unchanged. Elements that compare equal retain their original order.
    /// The comparison function must define a total order, as for [`slice::sort_by`].
    pub fn from_sort_by<T>(
        items: &[T],
        mut compare: impl FnMut(&T, &T) -> Ordering,
    ) -> IndexNewToOldBuf {
        let mut new_to_old: Vec<_> = (0..items.len()).collect();
        new_to_old.sort_by(|&a, &b| compare(&items[a], &items[b]));
        IndexNewToOldBuf(new_to_old)
    }

    pub(crate) fn from_sort_unstable_by<T>(
        items: &[T],
        mut compare: impl FnMut(&T, &T) -> Ordering,
    ) -> IndexNewToOldBuf {
        let mut new_to_old: Vec<_> = (0..items.len()).collect();
        new_to_old.sort_unstable_by(|&a, &b| compare(&items[a], &items[b]));
        IndexNewToOldBuf(new_to_old)
    }

    /// Builds the inverse mapping from old positions to new positions.
    ///
    /// # Panics
    ///
    /// Panics if an index is out of bounds or occurs more than once.
    pub fn build_old_to_new(&self) -> Vec<usize> {
        let mut old_to_new = vec![usize::MAX; self.0.len()];
        for (new_index, &old_index) in self.0.iter().enumerate() {
            assert!(old_index < self.0.len(), "old index out of bounds");
            assert_eq!(old_to_new[old_index], usize::MAX, "duplicate old index");
            old_to_new[old_index] = new_index;
        }
        old_to_new
    }

    /// Reorders `items` so each new position contains the specified old item.
    ///
    /// # Panics
    ///
    /// Panics before changing `items` if the lengths differ, an index is out of bounds,
    /// or an index occurs more than once.
    pub fn apply_to<T>(&self, items: &mut [T]) {
        assert_eq!(self.0.len(), items.len(), "permutation length mismatch");
        let mut old_to_new = self.build_old_to_new();
        for old in 0..items.len() {
            loop {
                let new = old_to_new[old];
                if old == new {
                    break;
                }
                items.swap(old, new);
                old_to_new.swap(old, new);
            }
        }
    }
    pub fn as_slice(&self) -> &[usize] {
        &self.0
    }
}

/// Owns a permutation and dereferences to [`IndexNewToOld`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexNewToOldBuf(Vec<usize>);

impl Deref for IndexNewToOldBuf {
    type Target = IndexNewToOld;

    fn deref(&self) -> &Self::Target {
        IndexNewToOld::new(&self.0)
    }
}

impl Borrow<IndexNewToOld> for IndexNewToOldBuf {
    fn borrow(&self) -> &IndexNewToOld {
        self
    }
}

impl ToOwned for IndexNewToOld {
    type Owned = IndexNewToOldBuf;

    fn to_owned(&self) -> Self::Owned {
        IndexNewToOldBuf(self.0.to_vec())
    }
}
impl std::ops::Index<usize> for IndexNewToOld {
    type Output = usize;
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

struct Change<T> {
    data: T,
    ref_count: usize,
}

pub(crate) struct Changes<T> {
    age_base: usize,
    items: VecDeque<Change<T>>,
    end_ref_count: usize,
}
impl<T> Changes<T> {
    pub fn new() -> Self {
        Self {
            age_base: 0,
            items: VecDeque::new(),
            end_ref_count: 0,
        }
    }
    pub fn push(&mut self, data: T) {
        let ref_count = self.end_ref_count;
        self.end_ref_count = 0;
        self.items.push_back(Change { data, ref_count });
    }
    fn increment_ref_count(&mut self, count: usize) {
        self.end_ref_count = self
            .end_ref_count
            .checked_add(count)
            .expect("ref_count overflow");
    }
    fn increment_ref_count_at(&mut self, age: usize) {
        let ref_count = self.ref_count_mut(age);
        *ref_count = ref_count.checked_add(1).expect("ref_count overflow");
    }
    fn decrement_ref_count(&mut self, age: usize) {
        let ref_count = self.ref_count_mut(age);
        *ref_count = ref_count
            .checked_sub(1)
            .expect("too many calls to `decrement_ref_count`");
    }
    fn ref_count_mut(&mut self, age: usize) -> &mut usize {
        let index = self.age_to_index(age);
        if index == self.items.len() {
            &mut self.end_ref_count
        } else {
            &mut self.items[index].ref_count
        }
    }
    pub fn end_age(&self) -> usize {
        self.age_base.wrapping_add(self.items.len())
    }
    pub fn age_to_index(&self, age: usize) -> usize {
        let index = age.wrapping_sub(self.age_base);
        assert!(index <= self.items.len());
        index
    }
    pub fn clean(&mut self, mut f: impl FnMut(T)) {
        while let Some(change) = self.items.front() {
            if change.ref_count != 0 {
                return;
            }
            let entry = self.items.pop_front().unwrap();
            self.age_base = self.age_base.wrapping_add(1);
            f(entry.data);
        }
    }
    pub fn items(&self, age: usize) -> impl Iterator<Item = &'_ T> {
        let index = self.age_to_index(age);
        self.items.iter().skip(index).map(|x| &x.data)
    }
}

pub(crate) struct RefCountOps {
    increments: usize,
    increment_ages: Vec<usize>,
    decrement_ages: Vec<usize>,
}

impl RefCountOps {
    pub fn new() -> Self {
        Self {
            increments: 0,
            increment_ages: Vec::new(),
            decrement_ages: Vec::new(),
        }
    }
    pub fn increment(&mut self) {
        self.increments += 1;
    }
    pub fn increment_at(&mut self, age: usize) {
        self.increment_ages.push(age);
    }
    pub fn decrement(&mut self, age: Option<usize>) {
        if let Some(age) = age {
            self.decrement_ages.push(age);
        }
    }
    pub fn apply<T>(&mut self, items: &mut Changes<T>) {
        items.increment_ref_count(self.increments);
        self.increments = 0;
        while let Some(age) = self.increment_ages.pop() {
            items.increment_ref_count_at(age);
        }
        while let Some(age) = self.decrement_ages.pop() {
            items.decrement_ref_count(age);
        }
    }
}

pub(crate) fn to_range(range: impl RangeBounds<usize>, len: usize) -> Range<usize> {
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

#[cfg(test)]
mod tests;
