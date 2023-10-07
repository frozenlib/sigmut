use std::{
    cell::{Ref, RefCell},
    rc::Rc,
};

use derive_ex::derive_ex;
use slabmap::SlabMap;

use crate::{
    core::{BindSource, SinkBindings, UpdateContext},
    ActionContext, ObsContext,
};

use crate::collections::utils::{ChangeLogs, RefCountLogs};

struct Bindings {
    items: Vec<SinkBindings>,
    all: SinkBindings,
}
impl Bindings {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            all: SinkBindings::new(),
        }
    }
    fn notify(&mut self, key: usize, uc: &mut UpdateContext) {
        self.all.notify(true, uc);
        if let Some(b) = self.items.get_mut(key) {
            b.notify(true, uc);
        }
    }
    fn watch(&mut self, this: Rc<dyn BindSource>, key: Option<usize>, oc: &mut ObsContext) {
        if let Some(key) = key {
            if self.items.len() < key {
                self.items.resize_with(key + 1, SinkBindings::new);
            }
            self.items[key].watch(this, key, oc);
        } else {
            self.all.watch(this, usize::MAX, oc);
        }
    }
    fn get_mut(&mut self, slot: usize) -> &mut SinkBindings {
        if slot == usize::MAX {
            &mut self.all
        } else {
            &mut self.items[slot]
        }
    }
}

struct ObsSlabMapItemsMut<T> {
    items: SlabMap<Item<T>>,
    len: usize,
    changes: ChangeLogs<ChangeData>,
}
impl<T> ObsSlabMapItemsMut<T> {
    fn new() -> Self {
        Self {
            items: SlabMap::new(),
            len: 0,
            changes: ChangeLogs::new(),
        }
    }
    fn get(&self, key: usize) -> Option<&T> {
        let item = self.items.get(key)?;
        if item.is_exists {
            Some(&item.value)
        } else {
            None
        }
    }
    fn push_change(&mut self, ref_counts: &mut RefCountLogs, change: ChangeData) {
        ref_counts.apply(&mut self.changes);
        self.changes.push(change);
        self.clean_changes();
    }
    fn clean_changes(&mut self) {
        self.changes.clean(|change| match change {
            ChangeData::Insert { .. } => {}
            ChangeData::Remove { key } => {
                self.items.remove(key);
            }
        });
    }
}

struct RawObsSlabMapCell<T> {
    data: RefCell<ObsSlabMapItemsMut<T>>,
    bindings: RefCell<Bindings>,
    ref_counts: RefCell<RefCountLogs>,
}

#[derive_ex(Default, Clone(bound()))]
#[default(Self::new())]
pub struct ObsSlabMapCell<T>(Rc<RawObsSlabMapCell<T>>);

impl<T> ObsSlabMapCell<T> {
    pub fn new() -> Self {
        Self(Rc::new(RawObsSlabMapCell {
            data: RefCell::new(ObsSlabMapItemsMut::new()),
            bindings: RefCell::new(Bindings::new()),
            ref_counts: RefCell::new(RefCountLogs::new()),
        }))
    }
}

impl<T: 'static> ObsSlabMapCell<T> {
    pub fn insert(&self, value: T, ac: &mut ActionContext) -> usize {
        let mut data = self.0.data.borrow_mut();
        let key = data.items.insert(Item::new(value));
        data.len += 1;
        data.push_change(
            &mut self.0.ref_counts.borrow_mut(),
            ChangeData::Insert { key },
        );
        self.0.bindings.borrow_mut().notify(key, ac.uc());
        key
    }
    pub fn remove(&self, key: usize, ac: &mut ActionContext) {
        let mut data = self.0.data.borrow_mut();
        let item = &mut data.items[key];
        assert!(item.is_exists);
        item.is_exists = false;
        data.len -= 1;
        data.push_change(
            &mut self.0.ref_counts.borrow_mut(),
            ChangeData::Remove { key },
        );
        self.0.bindings.borrow_mut().notify(key, ac.uc());
    }
    pub fn item(&self, key: usize, oc: &mut ObsContext) -> Ref<T> {
        self.watch(Some(key), oc);
        Ref::map(self.0.data.borrow(), |r| r.get(key).expect("key not found"))
    }
    pub fn items(&self, oc: &mut ObsContext) -> ObsSlabMapItems<T> {
        self.watch(None, oc);
        let data = self.0.data.borrow();
        let age = Some(data.changes.end_age());
        ObsSlabMapItems { data, age }
    }
    pub fn session(&self) -> ObsSlabMapSession<T> {
        ObsSlabMapSession {
            source: self.clone(),
            age: None,
        }
    }

    fn watch(&self, key: Option<usize>, oc: &mut ObsContext) {
        self.0.bindings.borrow_mut().watch(self.0.clone(), key, oc);
    }
}
impl<T: 'static> BindSource for RawObsSlabMapCell<T> {
    fn flush(self: Rc<Self>, _slot: usize, _uc: &mut UpdateContext) -> bool {
        false
    }
    fn unbind(self: Rc<Self>, slot: usize, key: usize, _uc: &mut UpdateContext) {
        self.bindings.borrow_mut().get_mut(slot).unbind(key)
    }
}

pub struct ObsSlabMapSession<T> {
    source: ObsSlabMapCell<T>,
    age: Option<usize>,
}
impl<T: 'static> ObsSlabMapSession<T> {
    pub fn read(&mut self, oc: &mut ObsContext) -> ObsSlabMapItems<T> {
        let age = self.age;

        let mut ref_counts = self.source.0.ref_counts.borrow_mut();
        ref_counts.increment();
        ref_counts.decrement(age);

        let data = self.source.0.data.borrow();
        self.age = Some(data.changes.end_age());
        self.source.watch(None, oc);
        ObsSlabMapItems { data, age }
    }
}
impl<T> Drop for ObsSlabMapSession<T> {
    fn drop(&mut self) {
        self.source.0.ref_counts.borrow_mut().decrement(self.age);
    }
}

pub struct ObsSlabMapItems<'a, T> {
    data: Ref<'a, ObsSlabMapItemsMut<T>>,
    age: Option<usize>,
}

impl<'a, T> ObsSlabMapItems<'a, T> {
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn len(&self) -> usize {
        self.data.len
    }
    pub fn get(&self, key: usize) -> Option<&T> {
        self.data.get(key)
    }
    pub fn iter(&self) -> ObsSlabMapIter<T> {
        ObsSlabMapIter(self.data.items.iter())
    }
    pub fn changes(&self, f: impl Fn(ObsSlabMapChange<T>)) {
        if let Some(age) = self.age {
            for change in self.data.changes.changes(age) {
                f(match change {
                    ChangeData::Insert { key } => ObsSlabMapChange::Insert {
                        key: *key,
                        new_value: &self.data.items[*key].value,
                    },
                    ChangeData::Remove { key } => ObsSlabMapChange::Remove {
                        key: *key,
                        old_value: &self.data.items[*key].value,
                    },
                });
            }
        } else {
            for (key, value) in self {
                f(ObsSlabMapChange::Insert {
                    key,
                    new_value: value,
                })
            }
        }
    }
}
impl<'a, T> std::ops::Index<usize> for ObsSlabMapItems<'a, T> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("index out of bounds")
    }
}

impl<'a, T> IntoIterator for &'a ObsSlabMapItems<'a, T> {
    type Item = (usize, &'a T);
    type IntoIter = ObsSlabMapIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
pub struct ObsSlabMapIter<'a, T>(slabmap::Iter<'a, Item<T>>);

impl<'a, T> Iterator for ObsSlabMapIter<'a, T> {
    type Item = (usize, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        for (key, value) in self.0.by_ref() {
            if value.is_exists {
                return Some((key, &value.value));
            }
        }
        None
    }
}

pub enum ObsSlabMapChange<'a, T> {
    Insert { key: usize, new_value: &'a T },
    Remove { key: usize, old_value: &'a T },
}

enum ChangeData {
    Insert { key: usize },
    Remove { key: usize },
}

struct Item<T> {
    value: T,
    is_exists: bool,
}
impl<T> Item<T> {
    fn new(value: T) -> Self {
        Self {
            value,
            is_exists: true,
        }
    }
}
