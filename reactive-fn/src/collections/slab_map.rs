use std::{
    cell::{Ref, RefCell, RefMut},
    rc::Rc,
};

use derive_ex::derive_ex;
use slabmap::SlabMap;

use crate::{
    core::{BindSource, SinkBindings, UpdateContext},
    ActionContext, ObsContext,
};

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
    fn watch(&mut self, this: Rc<dyn BindSource>, key: usize, oc: &mut ObsContext) {
        self.watch_all(this.clone(), oc);
        if self.items.len() < key {
            self.items.resize_with(key + 1, SinkBindings::new);
        }
        self.items[key].watch(this, key, oc);
    }
    fn watch_all(&mut self, this: Rc<dyn BindSource>, oc: &mut ObsContext) {
        self.all.watch(this, usize::MAX, oc);
    }
    fn get_mut(&mut self, slot: usize) -> &mut SinkBindings {
        if slot == usize::MAX {
            &mut self.all
        } else {
            &mut self.items[slot]
        }
    }
}

struct RawObsSlabMapCell<T> {
    items: RefCell<SlabMap<T>>,
    bindings: RefCell<Bindings>,
}

#[derive_ex(Default, Clone(bound()))]
#[default(Self::new())]
pub struct ObsSlabMapCell<T>(Rc<RawObsSlabMapCell<T>>);

impl<T> ObsSlabMapCell<T> {
    pub fn new() -> Self {
        Self(Rc::new(RawObsSlabMapCell {
            items: RefCell::new(SlabMap::new()),
            bindings: RefCell::new(Bindings::new()),
        }))
    }
}

impl<T: 'static> ObsSlabMapCell<T> {
    pub fn insert(&self, value: T, ac: &mut ActionContext) -> usize {
        let key = self.0.items.borrow_mut().insert(value);
        self.0.bindings.borrow_mut().notify(key, ac.uc());
        key
    }
    pub fn set(&self, key: usize, value: T, ac: &mut ActionContext) {
        self.0.items.borrow_mut()[key] = value;
        self.0.bindings.borrow_mut().notify(key, ac.uc());
    }
    pub fn remove(&self, key: usize, ac: &mut ActionContext) {
        self.0.items.borrow_mut().remove(key);
        self.0.bindings.borrow_mut().notify(key, ac.uc());
    }
    pub fn get(&self, key: usize, oc: &mut ObsContext) -> Ref<T> {
        self.0.bindings.borrow_mut().watch(self.0.clone(), key, oc);
        Ref::map(self.0.items.borrow(), |r| &r[key])
    }
    pub fn get_mut(&self, key: usize, ac: &mut ActionContext) -> RefMut<T> {
        self.0.bindings.borrow_mut().notify(key, ac.uc());
        RefMut::map(self.0.items.borrow_mut(), |r| &mut r[key])
    }
    pub fn items(&self, oc: &mut ObsContext) -> ObsSlabMapCellItems<T> {
        self.0.bindings.borrow_mut().watch_all(self.0.clone(), oc);
        ObsSlabMapCellItems(self.0.items.borrow())
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

pub struct ObsSlabMapCellItems<'a, T>(Ref<'a, SlabMap<T>>);

impl<'a, T> IntoIterator for &'a ObsSlabMapCellItems<'a, T> {
    type Item = (usize, &'a T);
    type IntoIter = slabmap::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

pub enum ObsSlabMapChange<'a, T> {
    Insert {
        key: usize,
        new_value: &'a T,
    },
    Remove {
        key: usize,
        old_value: &'a T,
    },
    Set {
        key: usize,
        old_value: &'a T,
        new_value: &'a T,
    },
}
