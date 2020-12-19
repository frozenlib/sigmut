use super::*;
use crate::*;
use std::{cell::RefCell, rc::Rc};

pub trait Collect: 'static {
    type Input;
    type Output;
    type Key;
    fn insert(&mut self, value: Self::Input) -> (Self::Key, bool);
    fn remove(&mut self, key: Self::Key) -> bool;
    fn set(&mut self, key: Self::Key, value: Self::Input) -> (Self::Key, bool);
    fn borrow(&self) -> &Self::Output;
}

pub trait ObservableCollect {
    type Observer: Observer<Item = Self::Item>;
    type Item;

    fn insert(&self, value: Self::Item) -> Self::Observer;
}

pub struct ObsCollector<T>(Rc<ObsCollectorData<T>>);
struct ObsCollectorData<T> {
    collector: RefCell<T>,
    sinks: BindSinks,
}

pub struct ObsCollectorObserver<T: Collect> {
    collector: Rc<ObsCollectorData<T>>,
    key: Option<T::Key>,
}
impl<T: Collect> ObsCollector<T> {
    pub fn as_dyn(&self) -> DynObsBorrow<T::Output> {
        DynObsBorrow::from_dyn_source(self.0.clone())
    }
    pub fn as_dyn_ref(&self) -> DynObsRef<T::Output> {
        self.as_dyn().as_ref()
    }
    pub fn obs(&self) -> ObsBorrow<impl ObservableBorrow<Item = T::Output> + Clone> {
        ObsBorrow(self.clone())
    }
    pub fn obs_ref(&self) -> ObsRef<impl ObservableRef<Item = T::Output> + Clone> {
        self.obs().as_ref()
    }
}
impl<T: Collect> ObservableBorrow for ObsCollector<T> {
    type Item = T::Output;
    fn borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.0.borrow(cx)
    }
}

impl<T: Collect> ObservableCollect for ObsCollector<T> {
    type Observer = ObsCollectorObserver<T>;
    type Item = T::Input;
    fn insert(&self, value: Self::Item) -> Self::Observer {
        let (key, is_modified) = self.0.collector.borrow_mut().insert(value);
        if is_modified {
            Runtime::notify_defer(self.0.clone());
        }
        ObsCollectorObserver {
            collector: self.0.clone(),
            key: Some(key),
        }
    }
}
impl<T> Clone for ObsCollector<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Collect> ObsCollectorData<T> {
    pub fn borrow<'a>(self: &'a Rc<Self>, cx: &BindContext<'a>) -> Ref<'a, T::Output> {
        cx.bind(self.clone());
        Ref::map(self.collector.borrow(), |c| c.borrow())
    }
}
impl<T: Collect> DynamicObservableBorrowSource for ObsCollectorData<T> {
    type Item = T::Output;
    fn dyn_borrow<'a>(
        &'a self,
        rc_self: &Rc<dyn DynamicObservableBorrowSource<Item = Self::Item>>,
        cx: &BindContext<'a>,
    ) -> Ref<'a, Self::Item> {
        cx.bind(Self::downcast(rc_self));
        Ref::map(self.collector.borrow(), |d| d.borrow())
    }
    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRefSource<Item = Self::Item>> {
        todo!()
    }
}
impl<T: Collect> DynamicObservableRefSource for ObsCollectorData<T> {
    type Item = T::Output;
    fn dyn_with(self: Rc<Self>, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
        f(&self.borrow(cx), cx)
    }
}
impl<T: 'static> BindSource for ObsCollectorData<T> {
    fn sinks(&self) -> &BindSinks {
        &self.sinks
    }
}

impl<T: Collect> Observer for ObsCollectorObserver<T> {
    type Item = T::Input;

    fn next(&mut self, value: Self::Item) {
        let (key, is_modified) = self
            .collector
            .collector
            .borrow_mut()
            .set(self.key.take().unwrap(), value);
        self.key = Some(key);
        if is_modified {
            Runtime::notify_defer(self.collector.clone());
        }
    }
}
impl<T: Collect> Drop for ObsCollectorObserver<T> {
    fn drop(&mut self) {
        if self
            .collector
            .collector
            .borrow_mut()
            .remove(self.key.take().unwrap())
        {
            Runtime::notify_defer(self.collector.clone());
        }
    }
}

// struct AnyCollecter {
//     count: usize,
//     count_base: usize,
//     result: bool,
// }
// impl Collect for AnyCollecter {
//     type Input = bool;
//     type Output = bool;
//     type Key = bool;

//     fn insert(&mut self, value: Self::Input) -> Self::Key {
//         if value {
//             self.count += 1;
//         }
//         value
//     }

//     fn remove(&mut self, key: Self::Key) {
//         if key {
//             self.count -= 1;
//         }
//     }

//     fn update(&mut self, key: &mut Self::Key, value: Self::Input) {
//         if *key {
//             self.count -= 1;
//         }
//         if value {
//             self.count += 1;
//         }
//         *key = value
//     }

//     fn is_modified(&self) -> bool {
//         self.count_base == self.count
//     }

//     fn collect(&mut self) -> bool {
//         let old = self.result;
//         self.result = self.count != 0;
//         self.result != old
//     }
//     fn result(&self) -> &Self::Output {
//         &self.result
//     }
// }
