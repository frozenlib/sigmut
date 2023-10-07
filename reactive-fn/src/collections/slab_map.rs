use std::{
    any::Any,
    cell::{Ref, RefCell, RefMut},
    ops::Index,
    rc::Rc,
};

use derive_ex::derive_ex;
use slabmap::SlabMap;

use crate::{
    collections::utils::{Changes, RefCountOps},
    core::{BindSink, BindSource, Computed, SinkBindings, SourceBindings, UpdateContext},
    ActionContext, ObsContext,
};

#[derive(Clone, Copy, Debug)]
pub enum ObsSlabMapChange<'a, T> {
    Insert { key: usize, new_value: &'a T },
    Remove { key: usize, old_value: &'a T },
}

enum ChangeAction {
    Insert,
    Remove,
}

struct ChangeData {
    action: ChangeAction,
    key: usize,
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

struct SinkBindingsSet {
    items: Vec<SinkBindings>,
    any: SinkBindings,
}
impl SinkBindingsSet {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            any: SinkBindings::new(),
        }
    }
    fn notify(&mut self, key: Option<usize>, uc: &mut UpdateContext) {
        if let Some(key) = key {
            if let Some(b) = self.items.get_mut(key) {
                b.notify(true, uc);
            }
        } else {
            self.any.notify(true, uc);
        }
    }
    fn notify_may_be_modified(&mut self, uc: &mut UpdateContext) {
        for b in &mut self.items {
            b.notify(false, uc);
        }
        self.any.notify(false, uc);
    }

    fn watch(&mut self, this: Rc<dyn BindSource>, key: Option<usize>, oc: &mut ObsContext) {
        if let Some(key) = key {
            if self.items.len() < key {
                self.items.resize_with(key + 1, SinkBindings::new);
            }
            self.items[key].watch(this, key, oc);
        } else {
            self.any.watch(this, usize::MAX, oc);
        }
    }
    fn get_mut(&mut self, slot: usize) -> &mut SinkBindings {
        if slot == usize::MAX {
            &mut self.any
        } else {
            &mut self.items[slot]
        }
    }
}

trait ObservableSlabMap<T> {
    fn to_any(self: Rc<Self>) -> Rc<dyn Any>;
    fn item(&self, this: Rc<dyn Any>, key: usize, oc: &mut ObsContext) -> Ref<T>;
    fn items(
        &self,
        this: Rc<dyn Any>,
        age: Option<usize>,
        oc: &mut ObsContext,
    ) -> ObsSlabMapItems<T>;
    fn ref_counts(&self) -> RefMut<RefCountOps>;
}

pub struct ObsSlabMapItemsMut<T> {
    items: SlabMap<Item<T>>,
    len: usize,
    changes: Changes<ChangeData>,
}
impl<T> ObsSlabMapItemsMut<T> {
    fn new() -> Self {
        Self {
            items: SlabMap::new(),
            len: 0,
            changes: Changes::new(),
        }
    }
    fn edit_start(&mut self, ref_counts: &RefCell<RefCountOps>) -> usize {
        ref_counts.borrow_mut().apply(&mut self.changes);
        self.clean_changes();
        self.changes.end_age()
    }
    fn edit_end(
        &mut self,
        sinks: &RefCell<SinkBindingsSet>,
        age: usize,
        uc: &mut UpdateContext,
    ) -> bool {
        let mut s = sinks.borrow_mut();
        let mut is_changed = false;
        for c in self.changes.changes(age) {
            is_changed = true;
            s.notify(Some(c.key), uc);
        }
        if is_changed {
            s.notify(None, uc);
        }
        self.clean_changes();
        is_changed
    }

    pub fn get(&self, key: usize) -> Option<&T> {
        let item = self.items.get(key)?;
        if item.is_exists {
            Some(&item.value)
        } else {
            None
        }
    }
    pub fn insert(&mut self, value: T) -> usize {
        let key = self.items.insert(Item::new(value));
        self.len += 1;
        self.changes.push(ChangeData {
            action: ChangeAction::Insert,
            key,
        });
        key
    }
    pub fn remove(&mut self, key: usize) {
        let item = &mut self.items[key];
        assert!(item.is_exists);
        item.is_exists = false;
        self.len -= 1;
        self.changes.push(ChangeData {
            action: ChangeAction::Remove,
            key,
        });
    }

    fn clean_changes(&mut self) {
        self.changes.clean(|d| match d.action {
            ChangeAction::Insert => {}
            ChangeAction::Remove => {
                self.items.remove(d.key);
            }
        });
    }
}
impl<T> Index<usize> for ObsSlabMapItemsMut<T> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("index out of bounds")
    }
}

#[derive_ex(Default, Clone(bound()))]
#[default(Self::new())]
pub struct ObsSlabMapCell<T>(Rc<RawObsSlabMapCell<T>>);

impl<T> ObsSlabMapCell<T> {
    pub fn new() -> Self {
        Self(Rc::new(RawObsSlabMapCell {
            items: RefCell::new(ObsSlabMapItemsMut::new()),
            bindings: RefCell::new(SinkBindingsSet::new()),
            ref_counts: RefCell::new(RefCountOps::new()),
        }))
    }
}

impl<T: 'static> ObsSlabMapCell<T> {
    pub fn obs(&self) -> ObsSlabMap<T> {
        ObsSlabMap(self.0.clone())
    }
    pub fn insert(&self, value: T, ac: &mut ActionContext) -> usize {
        let mut data = self.0.items.borrow_mut();
        let age = data.edit_start(&self.0.ref_counts);
        let key = data.insert(value);
        data.edit_end(&self.0.bindings, age, ac.uc());
        key
    }
    pub fn remove(&self, key: usize, ac: &mut ActionContext) {
        let mut data = self.0.items.borrow_mut();
        let age = data.edit_start(&self.0.ref_counts);
        data.remove(key);
        data.edit_end(&self.0.bindings, age, ac.uc());
    }
    pub fn item<'a, 'oc: 'a>(&'a self, key: usize, oc: &mut ObsContext<'oc>) -> Ref<'a, T> {
        self.0.watch(Some(key), oc);
        self.0.item(key)
    }
    pub fn items<'a, 'oc: 'a>(&'a self, oc: &mut ObsContext<'oc>) -> ObsSlabMapItems<'a, T> {
        self.0.watch(None, oc);
        self.0.items(None)
    }
    pub fn session(&self) -> ObsSlabMapSession<T> {
        ObsSlabMapSession::new(self.0.clone())
    }
}

struct RawObsSlabMapCell<T> {
    items: RefCell<ObsSlabMapItemsMut<T>>,
    bindings: RefCell<SinkBindingsSet>,
    ref_counts: RefCell<RefCountOps>,
}
impl<T: 'static> RawObsSlabMapCell<T> {
    fn rc_this(this: Rc<dyn Any>) -> Rc<Self> {
        Rc::downcast(this).unwrap()
    }
    fn watch(self: &Rc<Self>, key: Option<usize>, oc: &mut ObsContext) {
        self.bindings.borrow_mut().watch(self.clone(), key, oc);
    }
    fn item(&self, key: usize) -> Ref<T> {
        Ref::map(self.items.borrow(), |r| r.get(key).expect("key not found"))
    }
    fn items(&self, age: Option<usize>) -> ObsSlabMapItems<T> {
        let items = self.items.borrow();
        ObsSlabMapItems { items, age }
    }
}

impl<T: 'static> ObservableSlabMap<T> for RawObsSlabMapCell<T> {
    fn to_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }

    fn item(&self, this: Rc<dyn Any>, key: usize, oc: &mut ObsContext) -> Ref<T> {
        Self::rc_this(this).watch(Some(key), oc);
        self.item(key)
    }

    fn items(
        &self,
        this: Rc<dyn Any>,
        age: Option<usize>,
        oc: &mut ObsContext,
    ) -> ObsSlabMapItems<T> {
        Self::rc_this(this).watch(None, oc);
        self.items(age)
    }

    fn ref_counts(&self) -> RefMut<RefCountOps> {
        self.ref_counts.borrow_mut()
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
    owner: Rc<dyn ObservableSlabMap<T>>,
    age: Option<usize>,
}
impl<T: 'static> ObsSlabMapSession<T> {
    fn new(owner: Rc<dyn ObservableSlabMap<T>>) -> Self {
        Self { owner, age: None }
    }
    pub fn read<'a, 'oc: 'a>(&'a mut self, oc: &mut ObsContext<'oc>) -> ObsSlabMapItems<'a, T> {
        let age = self.age;

        let mut ref_counts = self.owner.ref_counts();
        ref_counts.increment();
        ref_counts.decrement(age);

        let items = self.owner.items(self.owner.clone().to_any(), age, oc);
        self.age = Some(items.items.changes.end_age());
        items
    }
}
impl<T> Drop for ObsSlabMapSession<T> {
    fn drop(&mut self) {
        self.owner.ref_counts().decrement(self.age);
    }
}

pub struct ObsSlabMapItems<'a, T> {
    items: Ref<'a, ObsSlabMapItemsMut<T>>,
    age: Option<usize>,
}

impl<'a, T> ObsSlabMapItems<'a, T> {
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn len(&self) -> usize {
        self.items.len
    }
    pub fn get(&self, key: usize) -> Option<&T> {
        self.items.get(key)
    }
    pub fn iter(&self) -> ObsSlabMapIter<T> {
        ObsSlabMapIter(self.items.items.iter())
    }
    pub fn changes(&self, f: impl Fn(ObsSlabMapChange<T>)) {
        if let Some(age) = self.age {
            for change in self.items.changes.changes(age) {
                let key = change.key;
                let value = &self.items.items[key].value;
                f(match change.action {
                    ChangeAction::Insert => ObsSlabMapChange::Insert {
                        key,
                        new_value: value,
                    },
                    ChangeAction::Remove => ObsSlabMapChange::Remove {
                        key,
                        old_value: value,
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
impl<'a, T> Index<usize> for ObsSlabMapItems<'a, T> {
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

pub struct ObsSlabMap<T>(Rc<dyn ObservableSlabMap<T>>);

impl<T: 'static> ObsSlabMap<T> {
    pub fn from_scan(f: impl FnMut(&mut ObsSlabMapItemsMut<T>, &mut ObsContext) + 'static) -> Self {
        Self(Scan::new(f))
    }

    pub fn item<'a, 'oc: 'a>(&'a self, key: usize, oc: &mut ObsContext<'oc>) -> Ref<'a, T> {
        self.0.item(self.0.clone().to_any(), key, oc)
    }
    pub fn items<'a, 'oc: 'a>(&'a self, oc: &mut ObsContext<'oc>) -> ObsSlabMapItems<'a, T> {
        self.0.items(self.0.clone().to_any(), None, oc)
    }
    pub fn session(&self) -> ObsSlabMapSession<T> {
        ObsSlabMapSession::new(self.0.clone())
    }
}

struct ScanData<T, F> {
    bindings: SourceBindings,
    items: ObsSlabMapItemsMut<T>,
    computed: Computed,
    f: F,
}

struct Scan<T, F> {
    data: RefCell<ScanData<T, F>>,
    ref_counts: RefCell<RefCountOps>,
    sinks: RefCell<SinkBindingsSet>,
}
impl<T, F> Scan<T, F>
where
    T: 'static,
    F: FnMut(&mut ObsSlabMapItemsMut<T>, &mut ObsContext) + 'static,
{
    fn new(f: F) -> Rc<Self> {
        Rc::new(Self {
            data: RefCell::new(ScanData {
                bindings: SourceBindings::new(),
                items: ObsSlabMapItemsMut::new(),
                computed: Computed::None,
                f,
            }),
            ref_counts: RefCell::new(RefCountOps::new()),
            sinks: RefCell::new(SinkBindingsSet::new()),
        })
    }

    fn update(self: &Rc<Self>, uc: &mut UpdateContext) -> bool {
        if self.data.borrow().computed == Computed::UpToDate {
            return false;
        }
        let this = Rc::downgrade(self);
        let d = &mut *self.data.borrow_mut();
        let age = d.items.edit_start(&self.ref_counts);
        d.bindings
            .compute(this, 0, |cc| (d.f)(&mut d.items, cc.oc()), uc);
        d.computed = Computed::UpToDate;
        d.items.edit_end(&self.sinks, age, uc)
    }
    fn rc_this(this: Rc<dyn Any>) -> Rc<Self> {
        Rc::downcast(this).unwrap()
    }
}

impl<T, F> ObservableSlabMap<T> for Scan<T, F>
where
    T: 'static,
    F: FnMut(&mut ObsSlabMapItemsMut<T>, &mut ObsContext) + 'static,
{
    fn to_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }

    fn item(&self, this: Rc<dyn Any>, key: usize, oc: &mut ObsContext) -> Ref<T> {
        let this = Self::rc_this(this);
        this.update(oc.uc());
        self.sinks.borrow_mut().watch(this, Some(key), oc);
        Ref::map(self.data.borrow(), |data| &data.items[key])
    }

    fn items(
        &self,
        this: Rc<dyn Any>,
        age: Option<usize>,
        oc: &mut ObsContext,
    ) -> ObsSlabMapItems<T> {
        let this = Self::rc_this(this);
        this.update(oc.uc());
        self.sinks.borrow_mut().watch(this, None, oc);
        let data = Ref::map(self.data.borrow(), |data| &data.items);
        ObsSlabMapItems { items: data, age }
    }
    fn ref_counts(&self) -> RefMut<RefCountOps> {
        self.ref_counts.borrow_mut()
    }
}
impl<T, F> BindSource for Scan<T, F>
where
    T: 'static,
    F: FnMut(&mut ObsSlabMapItemsMut<T>, &mut ObsContext) + 'static,
{
    fn flush(self: Rc<Self>, _slot: usize, uc: &mut UpdateContext) -> bool {
        if self.data.borrow().computed == Computed::MayBeOutdated {
            self.update(uc)
        } else {
            false
        }
    }

    fn unbind(self: Rc<Self>, slot: usize, key: usize, _uc: &mut UpdateContext) {
        self.sinks.borrow_mut().get_mut(slot).unbind(key);
    }
}
impl<T, F> BindSink for Scan<T, F>
where
    T: 'static,
    F: FnMut(&mut ObsSlabMapItemsMut<T>, &mut ObsContext) + 'static,
{
    fn notify(self: Rc<Self>, _slot: usize, is_modified: bool, uc: &mut UpdateContext) {
        if self.data.borrow_mut().computed.modify(is_modified) && !is_modified {
            self.sinks.borrow_mut().notify_may_be_modified(uc);
        }
    }
}
