use std::{
    mem::replace,
    ops::{Index, IndexMut},
};
pub struct VecArena<T> {
    entries: Vec<Entry<T>>,
    len: usize,
    index_vacant: usize,
}

enum Entry<T> {
    Vacant(usize),
    Occupied(T),
}

const INDEX_NONE: usize = usize::MAX;

impl<T> VecArena<T> {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            len: 0,
            index_vacant: INDEX_NONE,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }
    pub fn end(&self) -> usize {
        self.entries.len()
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        if let Some(Entry::Occupied(x)) = self.entries.get(index) {
            Some(x)
        } else {
            None
        }
    }
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        if let Some(Entry::Occupied(x)) = self.entries.get_mut(index) {
            Some(x)
        } else {
            None
        }
    }
    pub fn insert(&mut self, value: T) -> usize {
        let index;
        if self.index_vacant == INDEX_NONE {
            self.entries.push(Entry::Occupied(value));
            index = self.entries.len();
        } else {
            index = self.index_vacant;
            let e = &mut self.entries[index];
            if let &mut Entry::Vacant(index_vacant_next) = e {
                *e = Entry::Occupied(value);
                self.index_vacant = index_vacant_next;
            } else {
                unreachable!();
            }
        }
        self.len += 1;
        index
    }
    pub fn remove(&mut self, index: usize) -> Option<T> {
        let end = self.end();
        if let Some(e @ Entry::Occupied(_)) = self.entries.get_mut(index) {
            let e_value = replace(e, Entry::Vacant(self.index_vacant));
            if index + 1 == end {
                self.entries.remove(index);
            } else {
                *e = Entry::Vacant(self.index_vacant);
                self.index_vacant = index;
            }
            self.len -= 1;
            if let Entry::Occupied(value) = e_value {
                Some(value)
            } else {
                unreachable!()
            }
        } else {
            None
        }
    }
}

impl<T> Index<usize> for VecArena<T> {
    type Output = T;
    fn index(&self, index: usize) -> &T {
        self.get(index).expect("index out of bounds.")
    }
}
impl<T> IndexMut<usize> for VecArena<T> {
    fn index_mut(&mut self, index: usize) -> &mut T {
        self.get_mut(index).expect("index out of bounds.")
    }
}
