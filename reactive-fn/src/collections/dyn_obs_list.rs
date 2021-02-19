use super::*;
use crate::*;
use std::{any::Any, borrow::Borrow, ops::Index, rc::Rc};

pub(crate) trait DynamicObservableList<T> {
    fn borrow<'a>(
        &'a self,
        rs_self: &dyn Any,
        cx: &mut BindContext,
    ) -> Box<dyn DynamicObservableListRef<T> + 'a>;
}
pub(crate) trait DynamicObservableListRef<T> {
    fn age(&self) -> DynObsListAge;
    fn len(&self) -> usize;
    fn get(&self, index: usize) -> Option<&T>;
    fn changes(&self, since: &DynObsListAge, f: &mut dyn FnMut(ListChange<&T>));
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub struct DynObsList<T>(pub(crate) Rc<dyn DynamicObservableList<T>>);
pub struct DynObsListRef<'a, T>(Box<dyn DynamicObservableListRef<T> + 'a>);

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub enum DynObsListAge {
    Empty,
    Last,
    Obs(Rc<dyn Any>),
}

impl<T: 'static> DynObsList<T> {
    pub fn from_vec(values: Vec<T>) -> Self {
        Self(Rc::new(values))
    }

    pub fn borrow<'a>(&'a self, cx: &mut BindContext) -> DynObsListRef<'a, T> {
        DynObsListRef(DynamicObservableList::borrow(&*self.0, &self.0, cx))
    }

    pub fn map<U>(&self, f: impl Fn(&T) -> &U + 'static) -> DynObsList<U> {
        DynObsList(Rc::new(MapDynObsList { s: self.clone(), f }))
    }
    pub fn map_borrow<U: 'static>(&self) -> DynObsList<U>
    where
        T: Borrow<U>,
    {
        if let Some(b) = Any::downcast_ref::<DynObsList<U>>(self) {
            b.clone()
        } else {
            self.map(|x| x.borrow())
        }
    }
}
impl<T> DynObsListRef<'_, T> {
    pub fn age(&self) -> DynObsListAge {
        self.0.age()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, index: usize) -> Option<&T> {
        self.0.get(index)
    }
    pub fn changes(&self, since: &DynObsListAge, f: &mut dyn FnMut(ListChange<&T>)) {
        self.0.changes(since, f)
    }
    pub fn iter(&self) -> IndexIter<&Self> {
        IndexIter::new(self, 0, self.len())
    }
}
impl<T> Index<usize> for DynObsListRef<'_, T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("out of index.")
    }
}
impl<'a, T> IntoIterator for &'a DynObsListRef<'_, T> {
    type Item = &'a T;
    type IntoIter = IndexIter<Self>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

struct ConstantObsListRef<'a, T>(&'a [T]);

impl<T> DynamicObservableList<T> for Vec<T> {
    fn borrow<'a>(
        &'a self,
        _rs_self: &dyn Any,
        _cx: &mut BindContext,
    ) -> Box<dyn DynamicObservableListRef<T> + 'a> {
        Box::new(ConstantObsListRef(self.as_slice()))
    }
}
impl<'a, T> DynamicObservableListRef<T> for ConstantObsListRef<'a, T> {
    fn age(&self) -> DynObsListAge {
        DynObsListAge::Last
    }
    fn len(&self) -> usize {
        self.0.len()
    }
    fn get(&self, index: usize) -> Option<&T> {
        self.0.get(index)
    }
    fn changes(&self, since: &DynObsListAge, f: &mut dyn FnMut(ListChange<&T>)) {
        match since {
            DynObsListAge::Empty => list_change_for_each(self.0, f),
            DynObsListAge::Last => {}
            DynObsListAge::Obs(_) => {
                panic!("mismatch source.")
            }
        }
    }
}

struct MapDynObsList<T, F> {
    s: DynObsList<T>,
    f: F,
}
struct MapDynObsListRef<'a, T, F> {
    s: DynObsListRef<'a, T>,
    f: &'a F,
}

impl<T, U, F> DynamicObservableList<U> for MapDynObsList<T, F>
where
    T: 'static,
    F: Fn(&T) -> &U,
{
    fn borrow<'a>(
        &'a self,
        _rs_self: &dyn Any,
        cx: &mut BindContext,
    ) -> Box<dyn DynamicObservableListRef<U> + 'a> {
        Box::new(MapDynObsListRef {
            s: self.s.borrow(cx),
            f: &self.f,
        })
    }
}
impl<'a, T, U, F> DynamicObservableListRef<U> for MapDynObsListRef<'a, T, F>
where
    F: Fn(&T) -> &U,
{
    fn age(&self) -> DynObsListAge {
        self.s.age()
    }
    fn len(&self) -> usize {
        self.s.len()
    }
    fn get(&self, index: usize) -> Option<&U> {
        Some((self.f)(self.s.get(index)?))
    }
    fn changes(&self, since: &DynObsListAge, f: &mut dyn FnMut(ListChange<&U>)) {
        let m = &self.f;
        self.s.changes(since, &mut |c: ListChange<&T>| f(c.map(m)))
    }
}
