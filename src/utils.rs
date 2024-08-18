use std::{
    any::Any,
    collections::VecDeque,
    mem::transmute,
    ops::{Bound, Range, RangeBounds},
};

pub(crate) mod isize_map;
pub mod sync;

#[cfg(test)]
pub mod test_helpers;

pub(crate) fn downcast<T: 'static, S: 'static>(value: S) -> Result<T, S> {
    let mut value = Some(value);
    if let Some(value) = <dyn Any>::downcast_mut::<Option<T>>(&mut value) {
        Ok(value.take().unwrap())
    } else {
        Err(value.unwrap())
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

#[derive(Debug)]
#[repr(transparent)]
pub struct IndexNewToOld([usize]);
impl IndexNewToOld {
    pub fn new(new_to_old: &[usize]) -> &Self {
        unsafe { transmute(new_to_old) }
    }

    pub fn build_old_to_new(&self) -> Vec<usize> {
        let mut old_to_new = vec![usize::MAX; self.0.len()];
        for (new_index, &old_index) in self.0.iter().enumerate() {
            old_to_new[old_index] = new_index;
        }
        old_to_new
    }

    pub fn apply_to<T>(&self, items: &mut [T]) {
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
    fn decrement_ref_count(&mut self, age: usize) {
        let index = self.age_to_index(age);
        let ref_count = if index == self.items.len() {
            &mut self.end_ref_count
        } else {
            &mut self.items[index].ref_count
        };
        *ref_count = ref_count
            .checked_sub(1)
            .expect("too many calls to `decrement_ref_count`");
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
    decrement_ages: Vec<usize>,
}

impl RefCountOps {
    pub fn new() -> Self {
        Self {
            increments: 0,
            decrement_ages: Vec::new(),
        }
    }
    pub fn increment(&mut self) {
        self.increments += 1;
    }
    pub fn decrement(&mut self, age: Option<usize>) {
        if let Some(age) = age {
            self.decrement_ages.push(age);
        }
    }
    pub fn apply<T>(&mut self, items: &mut Changes<T>) {
        items.increment_ref_count(self.increments);
        self.increments = 0;
        while let Some(age) = self.decrement_ages.pop() {
            items.decrement_ref_count(age);
        }
    }
}

pub(crate) fn is_sorted(items: &[usize]) -> bool {
    for i in 1..items.len() {
        if items[i - 1] > items[i] {
            return false;
        }
    }
    true
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
