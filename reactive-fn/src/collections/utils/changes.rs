use std::collections::VecDeque;

struct Entry<T> {
    data: T,
    ref_count: usize,
}

pub struct Changes<T> {
    age_base: usize,
    changes: VecDeque<Entry<T>>,
    end_ref_count: usize,
}
impl<T> Changes<T> {
    pub fn new() -> Self {
        Self {
            age_base: 0,
            changes: VecDeque::new(),
            end_ref_count: 0,
        }
    }
    pub fn push(&mut self, data: T) {
        let ref_count = self.end_ref_count;
        self.end_ref_count = 0;
        self.changes.push_back(Entry { data, ref_count });
    }
    fn increment_ref_count(&mut self, count: usize) {
        self.end_ref_count = self
            .end_ref_count
            .checked_add(count)
            .expect("ref_count overflow");
    }
    fn decrement_ref_count(&mut self, age: usize) {
        let index = self.age_to_index(age);
        let ref_count = if index == self.changes.len() {
            &mut self.end_ref_count
        } else {
            &mut self.changes[index].ref_count
        };
        *ref_count = ref_count
            .checked_sub(1)
            .expect("too many calles to `decrement_ref_count`");
    }
    pub fn end_age(&self) -> usize {
        self.age_base.wrapping_add(self.changes.len())
    }
    pub fn age_to_index(&self, age: usize) -> usize {
        let index = age.wrapping_sub(self.age_base);
        assert!(index <= self.changes.len());
        index
    }
    pub fn clean(&mut self, mut f: impl FnMut(T)) {
        while let Some(change) = self.changes.front() {
            if change.ref_count != 0 {
                return;
            }
            let entry = self.changes.pop_front().unwrap();
            self.age_base = self.age_base.wrapping_add(1);
            f(entry.data);
        }
    }
    pub fn changes(&self, age: usize) -> impl Iterator<Item = &T> + '_ {
        let index = self.age_to_index(age);
        self.changes.iter().map(|x| &x.data).skip(index)
    }
}

pub struct RefCountOps {
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
