use crate::*;
use slabmap::SlabMap;
use std::{
    cell::{Ref, RefCell, RefMut},
    collections::VecDeque,
    mem::ManuallyDrop,
    ops::{Index, IndexMut},
    rc::{Rc, Weak},
};

pub struct ObsArray<T>(Rc<Inner<T>>);

pub struct ObsArrayAge<T> {
    source: Weak<Inner<T>>,
    age: usize,
}

pub struct ObsArrayRef<'a, T: 'static> {
    source: &'a Rc<Inner<T>>,
    state: ManuallyDrop<Ref<'a, State<T>>>,
}
pub struct ObsArrayRefMut<'a, T: 'static> {
    source: &'a Rc<Inner<T>>,
    state: ManuallyDrop<RefMut<'a, State<T>>>,
    logs_len_old: usize,
}

struct Inner<T> {
    state: RefCell<State<T>>,
    log_refs: RefCell<LogRefs>,
    sinks: BindSinks,
}

struct State<T> {
    data: SlabMap<Data<T>>,
    items: Vec<usize>,
    logs: VecDeque<Log>,
}
struct Data<T> {
    age_insert: Option<usize>,
    age_remove: Option<usize>,
    age_modify: Option<usize>,
    value: T,
}

struct LogRefs {
    counts: VecDeque<usize>,
    read: usize,
    base_age: usize,
}

struct Log {
    index: usize,
    data: usize,
    kind: ObsArrayChangeKind,
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum ObsArrayChangeKind {
    Insert,
    Remove,
    Modify,
}

pub struct ObsArrayChange<'a, T> {
    pub kind: ObsArrayChangeKind,

    /// Index of the changed element. (The index at the time the change was made.)
    pub index: usize,

    /// The most recent value, not the one immediately after it was changed.
    pub value: &'a T,
}
pub struct ObsArrayChanges<'a, T> {
    state: &'a State<T>,
    index: usize,
    age: usize,
}

impl<T: 'static> ObsArray<T> {
    pub fn new() -> Self {
        Self(Rc::new(Inner::new()))
    }
    pub fn borrow(&self, cx: &mut BindContext) -> ObsArrayRef<T> {
        cx.bind(self.0.clone());
        self.0.log_refs.borrow_mut().set_read();
        ObsArrayRef {
            source: &self.0,
            state: ManuallyDrop::new(self.0.state.borrow()),
        }
    }
    pub fn borrow_mut(&self) -> ObsArrayRefMut<T> {
        let state = ManuallyDrop::new(self.0.state.borrow_mut());
        let logs_len_old = state.logs.len();
        ObsArrayRefMut {
            source: &self.0,
            state,
            logs_len_old,
        }
    }
}
impl<T: 'static> Inner<T> {
    fn new() -> Self {
        Self {
            state: RefCell::new(State::new()),
            log_refs: RefCell::new(LogRefs::new()),
            sinks: BindSinks::new(),
        }
    }
    fn try_clean_logs(&self) {
        if let Ok(mut state) = self.state.try_borrow_mut() {
            clean(&mut state, &mut self.log_refs.borrow_mut());
        }
    }
}
fn clean<T>(state: &mut State<T>, log_refs: &mut LogRefs) {
    while log_refs.counts.len() > 1 && log_refs.counts[0] == 0 {
        log_refs.counts.pop_front();
        let log = state.logs.pop_front().unwrap();
        let d = &mut state.data[log.data];
        if d.age_modify == Some(log_refs.base_age) {
            d.age_modify = None;
        }
        match log.kind {
            ObsArrayChangeKind::Insert => {
                state.data[log.data].age_insert = None;
            }
            ObsArrayChangeKind::Remove => {
                state.data.remove(log.data);
            }
            ObsArrayChangeKind::Modify => {}
        }
        log_refs.base_age = log_refs.base_age.wrapping_add(1);
        log_refs.read = log_refs.read.saturating_sub(1);
    }
}

impl<T: 'static> BindSource for Inner<T> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}

impl<T> State<T> {
    fn new() -> Self {
        Self {
            data: SlabMap::new(),
            items: Vec::new(),
            logs: VecDeque::new(),
        }
    }
    fn get(&self, index: usize) -> Option<&T> {
        Some(&self.data[*self.items.get(index)?].value)
    }
}
impl LogRefs {
    fn new() -> Self {
        Self {
            counts: VecDeque::from(vec![0]),
            read: 0,
            base_age: 0,
        }
    }
    fn set_read(&mut self) {
        self.read = self.counts.len() - 1;
    }
    fn is_unread(&self, age: Option<usize>) -> bool {
        if let Some(age) = age {
            let idx = age.wrapping_sub(self.base_age);
            self.read <= idx
        } else {
            false
        }
    }
    fn age(&self) -> usize {
        self.base_age.wrapping_add(self.counts.len() - 1)
    }
    fn increment_last(&mut self) -> usize {
        let idx = self.counts.len() - 1;
        self.counts[idx] += 1;
        self.base_age.wrapping_add(idx)
    }
    fn increment(&mut self, age: usize) {
        self.counts[age.wrapping_sub(self.base_age)] += 1;
    }
    fn decrement(&mut self, age: usize) {
        self.counts[age.wrapping_sub(self.base_age)] -= 1;
    }
}

impl<T> Drop for ObsArrayAge<T> {
    fn drop(&mut self) {
        if let Some(s) = self.source.upgrade() {
            s.log_refs.borrow_mut().decrement(self.age);
        }
    }
}
impl<T> Clone for ObsArrayAge<T> {
    fn clone(&self) -> Self {
        if let Some(s) = self.source.upgrade() {
            s.log_refs.borrow_mut().increment(self.age);
        }
        Self {
            source: self.source.clone(),
            age: self.age,
        }
    }
}

impl<'a, T: 'static> ObsArrayRef<'a, T> {
    pub fn age(&self) -> ObsArrayAge<T> {
        ObsArrayAge {
            source: Rc::downgrade(self.source),
            age: self.source.log_refs.borrow_mut().increment_last(),
        }
    }
    pub fn len(&self) -> usize {
        self.state.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.state.items.is_empty()
    }
    pub fn get(&self, index: usize) -> Option<&T> {
        self.state.get(index)
    }
    pub fn changes(&self, age: &ObsArrayAge<T>) -> ObsArrayChanges<T> {
        if !Rc::downgrade(self.source).ptr_eq(&age.source) {
            panic!("mismatch source.");
        }
        let age = age.age;
        ObsArrayChanges {
            state: &self.state,
            index: age.wrapping_sub(self.source.log_refs.borrow().base_age),
            age,
        }
    }
}
impl<'a, T: 'static> Drop for ObsArrayRef<'a, T> {
    fn drop(&mut self) {
        unsafe { ManuallyDrop::drop(&mut self.state) }
        self.source.try_clean_logs()
    }
}
impl<'a, T> Index<usize> for ObsArrayRef<'a, T> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("out of index.")
    }
}
impl<'a, T> Iterator for ObsArrayChanges<'a, T> {
    type Item = ObsArrayChange<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let s = &*self.state;
            let log = s.logs.get(self.index)?;
            let age = self.age;
            self.index += 1;
            self.age += 1;
            if log.kind == ObsArrayChangeKind::Modify && s.data[log.data].age_modify != Some(age) {
                continue;
            }
            return Some(ObsArrayChange {
                kind: log.kind,
                index: log.index,
                value: &self.state.data[log.data].value,
            });
        }
    }
}

impl<'a, T> ObsArrayRefMut<'a, T> {
    pub fn len(&self) -> usize {
        self.state.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.state.items.is_empty()
    }
    pub fn get(&self, index: usize) -> Option<&T> {
        self.state.get(index)
    }
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        let s = &mut **self.state;
        let data = *s.items.get(index)?;
        let d = &mut s.data[data];
        let mut log_refs = self.source.log_refs.borrow_mut();
        if log_refs.is_unread(d.age_modify) {
            s.logs.push_back(Log {
                kind: ObsArrayChangeKind::Modify,
                index,
                data,
            });
            log_refs.counts.push_back(0);
        }
        Some(&mut d.value)
    }

    pub fn push(&mut self, value: T) {
        let index = self.state.items.len();
        self.insert(index, value);
    }
    pub fn insert(&mut self, index: usize, value: T) {
        let s = &mut **self.state;
        let mut log_refs = self.source.log_refs.borrow_mut();
        let age = log_refs.age();
        let data = s.data.insert(Data {
            value,
            age_insert: Some(age),
            age_remove: None,
            age_modify: Some(age),
        });
        s.items.insert(index, data);
        s.logs.push_back(Log {
            kind: ObsArrayChangeKind::Insert,
            index,
            data,
        });
        log_refs.counts.push_back(0);
    }
    pub fn remove(&mut self, index: usize) {
        let s = &mut **self.state;
        let mut log_refs = self.source.log_refs.borrow_mut();
        let data = s.items.remove(index);
        let age = log_refs.age();
        let d = &mut s.data[data];
        d.age_remove = Some(age);
        d.age_modify = Some(age);
        s.logs.push_back(Log {
            kind: ObsArrayChangeKind::Remove,
            index,
            data,
        });
        log_refs.counts.push_back(0);
    }
    pub fn clear(&mut self) {
        while !self.is_empty() {
            self.remove(0)
        }
    }
}
impl<'a, T> Drop for ObsArrayRefMut<'a, T> {
    fn drop(&mut self) {
        let logs_len = self.state.logs.len();
        unsafe { ManuallyDrop::drop(&mut self.state) }
        self.source.try_clean_logs();
        if self.logs_len_old != logs_len {
            Runtime::spawn_notify(self.source.clone());
        }
    }
}

impl<'a, T> Index<usize> for ObsArrayRefMut<'a, T> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("out of index.")
    }
}
impl<'a, T> IndexMut<usize> for ObsArrayRefMut<'a, T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.get_mut(index).expect("out of index.")
    }
}
