use std::{
    any::Any,
    cell::{Ref, RefCell, RefMut},
    ops::Index,
    rc::Rc,
};

use derive_ex::derive_ex;
use slabmap::SlabMap;

use crate::{
    core::{
        BindKey, BindSink, BindSource, DirtyOrMaybeDirty, NotifyContext, SinkBindings, Slot,
        SourceBinder, UpdateContext,
    },
    utils::{Changes, RefCountOps},
    ActionContext, SignalContext,
};

const SLOT_ITEMS: Slot = Slot(usize::MAX);

fn key_to_slot(key: usize) -> Slot {
    assert!(key != usize::MAX);
    Slot(key)
}
fn slot_to_key(slot: Slot) -> Option<usize> {
    if slot == SLOT_ITEMS {
        None
    } else {
        Some(slot.0)
    }
}

pub struct SignalSlabMap<T>(Rc<dyn DynSignalSlabMap<T>>);

impl<T: 'static> SignalSlabMap<T> {
    pub fn from_scan(f: impl FnMut(&mut ItemsMut<T>, &mut SignalContext) + 'static) -> Self {
        Self(Scan::new(f))
    }

    pub fn item<'a, 's: 'a>(&'a self, key: usize, sc: &mut SignalContext<'s>) -> Ref<'a, T> {
        self.0.item(self.0.clone().to_any(), key, sc)
    }
    pub fn items<'a, 's: 'a>(&'a self, sc: &mut SignalContext<'s>) -> Items<'a, T> {
        self.0.items(self.0.clone().to_any(), None, sc)
    }
    pub fn reader(&self) -> SignalSlabMapReader<T> {
        SignalSlabMapReader::new(self.0.clone())
    }
}

trait DynSignalSlabMap<T> {
    fn to_any(self: Rc<Self>) -> Rc<dyn Any>;
    fn item(&self, this: Rc<dyn Any>, key: usize, sc: &mut SignalContext) -> Ref<T>;
    fn items(&self, this: Rc<dyn Any>, age: Option<usize>, sc: &mut SignalContext) -> Items<T>;
    fn ref_counts(&self) -> RefMut<RefCountOps>;
}

pub struct SignalSlabMapReader<T> {
    owner: Rc<dyn DynSignalSlabMap<T>>,
    age: Option<usize>,
}
impl<T: 'static> SignalSlabMapReader<T> {
    fn new(owner: Rc<dyn DynSignalSlabMap<T>>) -> Self {
        Self { owner, age: None }
    }
    pub fn read<'a, 's: 'a>(&'a mut self, sc: &mut SignalContext<'s>) -> Items<'a, T> {
        let age = self.age;

        let mut ref_counts = self.owner.ref_counts();
        ref_counts.increment();
        ref_counts.decrement(age);

        let items = self.owner.items(self.owner.clone().to_any(), age, sc);
        self.age = Some(items.items.changes.end_age());
        items
    }
}
impl<T> Drop for SignalSlabMapReader<T> {
    fn drop(&mut self) {
        self.owner.ref_counts().decrement(self.age);
    }
}

pub struct Items<'a, T> {
    items: Ref<'a, ItemsMut<T>>,
    age: Option<usize>,
}

impl<'a, T> Items<'a, T> {
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn len(&self) -> usize {
        self.items.len
    }
    pub fn get(&self, key: usize) -> Option<&T> {
        self.items.get(key)
    }
    pub fn iter(&self) -> Iter<T> {
        Iter(self.items.items.iter())
    }
    pub fn changes(&self) -> impl Iterator<Item = SlabMapChange<T>> {
        use iter_n::iter2::*;
        if let Some(age) = self.age {
            self.items
                .changes
                .items(age)
                .map(|change| {
                    let key = change.key;
                    let value = &self.items.items[key].value;
                    match change.action {
                        ChangeAction::Insert => SlabMapChange::Insert {
                            key,
                            new_value: value,
                        },
                        ChangeAction::Remove => SlabMapChange::Remove {
                            key,
                            old_value: value,
                        },
                    }
                })
                .into_iter0()
        } else {
            self.iter()
                .map(|(key, value)| SlabMapChange::Insert {
                    key,
                    new_value: value,
                })
                .into_iter1()
        }
    }
}
impl<'a, T> Index<usize> for Items<'a, T> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("index out of bounds")
    }
}

impl<'a, T> IntoIterator for &'a Items<'a, T> {
    type Item = (usize, &'a T);
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct ItemsMut<T> {
    items: SlabMap<Item<T>>,
    len: usize,
    changes: Changes<ChangeData>,
}
impl<T> ItemsMut<T> {
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
        age: usize,
        sinks: &mut SinkBindingsSet,
        mut f: impl FnMut(&mut SinkBindings),
    ) {
        let mut is_dirty = false;
        for c in self.changes.items(age) {
            is_dirty = true;
            if let Some(sink) = sinks.get_mut(key_to_slot(c.key)) {
                f(sink);
            }
        }
        if is_dirty {
            f(&mut sinks.any);
        }
        self.clean_changes();
    }
    fn edit_end_and_update(
        &mut self,
        age: usize,
        sinks: &mut SinkBindingsSet,
        uc: &mut UpdateContext,
    ) {
        self.edit_end(age, sinks, |sink| sink.update(true, uc))
    }
    fn edit_end_and_notify(
        &mut self,
        sinks: &mut SinkBindingsSet,
        age: usize,
        nc: &mut NotifyContext,
    ) {
        self.edit_end(age, sinks, |sink| sink.notify(DirtyOrMaybeDirty::Dirty, nc))
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
impl<T> Index<usize> for ItemsMut<T> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("index out of bounds")
    }
}

pub struct Iter<'a, T>(slabmap::Iter<'a, Item<T>>);

impl<'a, T> Iterator for Iter<'a, T> {
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

#[derive(Clone, Copy, Debug)]
pub enum SlabMapChange<'a, T> {
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

#[derive_ex(Default, Clone(bound()))]
#[default(Self::new())]
pub struct StateSlabMap<T>(Rc<RawStateSlabMap<T>>);

impl<T> StateSlabMap<T> {
    pub fn new() -> Self {
        Self(Rc::new(RawStateSlabMap {
            items: RefCell::new(ItemsMut::new()),
            sinks: RefCell::new(SinkBindingsSet::new()),
            ref_counts: RefCell::new(RefCountOps::new()),
        }))
    }
}

impl<T: 'static> StateSlabMap<T> {
    pub fn to_signal_slab_map(&self) -> SignalSlabMap<T> {
        SignalSlabMap(self.0.clone())
    }
    pub fn insert(&self, value: T, ac: &mut ActionContext) -> usize {
        let mut data = self.0.items.borrow_mut();
        let age = data.edit_start(&self.0.ref_counts);
        let key = data.insert(value);
        data.edit_end_and_notify(&mut self.0.sinks.borrow_mut(), age, ac.nc());
        key
    }
    pub fn remove(&self, key: usize, ac: &mut ActionContext) {
        let mut data = self.0.items.borrow_mut();
        let age = data.edit_start(&self.0.ref_counts);
        data.remove(key);
        data.edit_end_and_notify(&mut self.0.sinks.borrow_mut(), age, ac.nc());
    }
    pub fn item<'a, 's: 'a>(&'a self, key: usize, sc: &mut SignalContext<'s>) -> Ref<'a, T> {
        self.0.bind(key_to_slot(key), sc);
        self.0.item(key)
    }
    pub fn items<'a, 's: 'a>(&'a self, sc: &mut SignalContext<'s>) -> Items<'a, T> {
        self.0.bind(SLOT_ITEMS, sc);
        self.0.items(None)
    }
    pub fn session(&self) -> SignalSlabMapReader<T> {
        SignalSlabMapReader::new(self.0.clone())
    }
}

struct RawStateSlabMap<T> {
    items: RefCell<ItemsMut<T>>,
    sinks: RefCell<SinkBindingsSet>,
    ref_counts: RefCell<RefCountOps>,
}
impl<T: 'static> RawStateSlabMap<T> {
    fn rc_this(this: Rc<dyn Any>) -> Rc<Self> {
        Rc::downcast(this).unwrap()
    }
    fn bind(self: &Rc<Self>, slot: Slot, sc: &mut SignalContext) {
        self.sinks.borrow_mut().bind(self.clone(), slot, sc);
    }
    fn item(&self, key: usize) -> Ref<T> {
        Ref::map(self.items.borrow(), |r| r.get(key).expect("key not found"))
    }
    fn items(&self, age: Option<usize>) -> Items<T> {
        let items = self.items.borrow();
        Items { items, age }
    }
}

impl<T: 'static> DynSignalSlabMap<T> for RawStateSlabMap<T> {
    fn to_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }

    fn item(&self, this: Rc<dyn Any>, key: usize, sc: &mut SignalContext) -> Ref<T> {
        Self::rc_this(this).bind(key_to_slot(key), sc);
        self.item(key)
    }

    fn items(&self, this: Rc<dyn Any>, age: Option<usize>, sc: &mut SignalContext) -> Items<T> {
        Self::rc_this(this).bind(SLOT_ITEMS, sc);
        self.items(age)
    }

    fn ref_counts(&self) -> RefMut<RefCountOps> {
        self.ref_counts.borrow_mut()
    }
}

impl<T: 'static> BindSource for RawStateSlabMap<T> {
    fn check(self: Rc<Self>, slot: Slot, key: BindKey, uc: &mut UpdateContext) -> bool {
        self.sinks.borrow_mut().is_dirty(slot, key, uc)
    }
    fn unbind(self: Rc<Self>, slot: Slot, key: BindKey, uc: &mut UpdateContext) {
        self.sinks.borrow_mut().unbind(slot, key, uc);
    }
    fn rebind(self: Rc<Self>, slot: Slot, key: BindKey, sc: &mut SignalContext) {
        self.sinks.borrow_mut().rebind(self.clone(), slot, key, sc);
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
    fn update_all(&mut self, is_dirty: bool, uc: &mut UpdateContext) {
        for s in &mut self.items {
            s.update(is_dirty, uc);
        }
        self.any.update(is_dirty, uc);
    }
    fn notify_all(&mut self, dirty: DirtyOrMaybeDirty, nc: &mut NotifyContext) {
        for s in &mut self.items {
            s.notify(dirty, nc);
        }
        self.any.notify(dirty, nc);
    }

    fn bind(&mut self, this: Rc<dyn BindSource>, slot: Slot, sc: &mut SignalContext) {
        if let Some(key) = slot_to_key(slot) {
            if self.items.len() < key {
                self.items.resize_with(key + 1, SinkBindings::new);
            }
        }
        if let Some(s) = self.get_mut(slot) {
            s.bind(this, slot, sc);
        }
    }
    fn unbind(&mut self, slot: Slot, key: BindKey, uc: &mut UpdateContext) {
        if let Some(s) = self.get_mut(slot) {
            s.unbind(key, uc);
        }
    }
    fn rebind(
        &mut self,
        this: Rc<dyn BindSource>,
        slot: Slot,
        key: BindKey,
        sc: &mut SignalContext,
    ) {
        if let Some(s) = self.get_mut(slot) {
            s.rebind(this, slot, key, sc);
        }
    }

    fn is_dirty(&self, slot: Slot, key: BindKey, uc: &mut UpdateContext) -> bool {
        if let Some(s) = self.get(slot) {
            s.is_dirty(key, uc)
        } else {
            true
        }
    }

    fn get(&self, slot: Slot) -> Option<&SinkBindings> {
        if let Some(key) = slot_to_key(slot) {
            self.items.get(key)
        } else {
            Some(&self.any)
        }
    }
    fn get_mut(&mut self, slot: Slot) -> Option<&mut SinkBindings> {
        if let Some(key) = slot_to_key(slot) {
            self.items.get_mut(key)
        } else {
            Some(&mut self.any)
        }
    }
}

struct Scan<T, F> {
    data: RefCell<ScanData<T, F>>,
    ref_counts: RefCell<RefCountOps>,
    sinks: RefCell<SinkBindingsSet>,
}
impl<T, F> Scan<T, F>
where
    T: 'static,
    F: FnMut(&mut ItemsMut<T>, &mut SignalContext) + 'static,
{
    fn new(f: F) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            data: RefCell::new(ScanData {
                sb: SourceBinder::new(this, Slot(0)),
                items: ItemsMut::new(),
                f,
            }),
            ref_counts: RefCell::new(RefCountOps::new()),
            sinks: RefCell::new(SinkBindingsSet::new()),
        })
    }

    fn update(self: &Rc<Self>, uc: &mut UpdateContext) {
        if uc.borrow(&self.data).sb.is_clean() {
            return;
        }
        let d = &mut *self.data.borrow_mut();
        if d.sb.check(uc) {
            let age = d.items.edit_start(&self.ref_counts);
            d.sb.update(|sc| (d.f)(&mut d.items, sc), uc);
            d.items
                .edit_end_and_update(age, &mut self.sinks.borrow_mut(), uc);
        }
        self.sinks.borrow_mut().update_all(false, uc);
    }
    fn rc_this(this: Rc<dyn Any>) -> Rc<Self> {
        Rc::downcast(this).unwrap()
    }
}

impl<T, F> DynSignalSlabMap<T> for Scan<T, F>
where
    T: 'static,
    F: FnMut(&mut ItemsMut<T>, &mut SignalContext) + 'static,
{
    fn to_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }

    fn item(&self, this: Rc<dyn Any>, key: usize, sc: &mut SignalContext) -> Ref<T> {
        let this = Self::rc_this(this);
        this.update(sc.uc());
        self.sinks.borrow_mut().bind(this, key_to_slot(key), sc);
        Ref::map(self.data.borrow(), |data| &data.items[key])
    }

    fn items(&self, this: Rc<dyn Any>, age: Option<usize>, sc: &mut SignalContext) -> Items<T> {
        let this = Self::rc_this(this);
        this.update(sc.uc());
        self.sinks.borrow_mut().bind(this, SLOT_ITEMS, sc);
        let data = Ref::map(self.data.borrow(), |data| &data.items);
        Items { items: data, age }
    }
    fn ref_counts(&self) -> RefMut<RefCountOps> {
        self.ref_counts.borrow_mut()
    }
}
impl<T, F> BindSource for Scan<T, F>
where
    T: 'static,
    F: FnMut(&mut ItemsMut<T>, &mut SignalContext) + 'static,
{
    fn check(self: Rc<Self>, slot: Slot, key: BindKey, uc: &mut UpdateContext) -> bool {
        self.update(uc);
        self.sinks.borrow().is_dirty(slot, key, uc)
    }

    fn unbind(self: Rc<Self>, slot: Slot, key: BindKey, uc: &mut UpdateContext) {
        self.sinks.borrow_mut().unbind(slot, key, uc)
    }

    fn rebind(self: Rc<Self>, slot: Slot, key: BindKey, sc: &mut SignalContext) {
        self.sinks.borrow_mut().rebind(self.clone(), slot, key, sc)
    }
}
impl<T, F> BindSink for Scan<T, F>
where
    T: 'static,
    F: FnMut(&mut ItemsMut<T>, &mut SignalContext) + 'static,
{
    fn notify(self: Rc<Self>, slot: Slot, dirty: DirtyOrMaybeDirty, nc: &mut NotifyContext) {
        if self.data.borrow_mut().sb.on_notify(slot, dirty) {
            self.sinks
                .borrow_mut()
                .notify_all(DirtyOrMaybeDirty::MaybeDirty, nc);
        }
    }
}

struct ScanData<T, F> {
    sb: SourceBinder,
    items: ItemsMut<T>,
    f: F,
}
