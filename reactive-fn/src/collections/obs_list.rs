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
    fn age(&self) -> ObsListAge;
    fn len(&self) -> usize;
    fn get(&self, index: usize) -> Option<&T>;
    fn changes(&self, since: &ObsListAge, f: &mut dyn FnMut(ListChange<&T>));
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub struct ObsList<T>(pub(crate) Rc<dyn DynamicObservableList<T>>);
pub struct ObsListRef<'a, T>(Box<dyn DynamicObservableListRef<T> + 'a>);

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub enum ObsListAge {
    Empty,
    Last,
    Obs(Rc<dyn Any>),
}

impl<T: 'static> ObsList<T> {
    pub fn from_vec(values: Vec<T>) -> Self {
        Self(Rc::new(values))
    }
    pub fn from_rc_vec(values: Rc<Vec<T>>) -> Self {
        Self(values)
    }

    pub fn borrow<'a>(&'a self, cx: &mut BindContext) -> ObsListRef<'a, T> {
        ObsListRef(DynamicObservableList::borrow(&*self.0, &self.0, cx))
    }

    pub fn map<U>(&self, f: impl Fn(&T) -> &U + 'static) -> ObsList<U> {
        ObsList(Rc::new(MapDynObsList { s: self.clone(), f }))
    }
    pub fn map_borrow<U: 'static>(&self) -> ObsList<U>
    where
        T: Borrow<U>,
    {
        if let Some(b) = <dyn Any>::downcast_ref::<ObsList<U>>(self) {
            b.clone()
        } else {
            self.map(|x| x.borrow())
        }
    }
}
impl<T> ObsListRef<'_, T> {
    pub fn age(&self) -> ObsListAge {
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
    pub fn changes(&self, since: &ObsListAge, f: &mut dyn FnMut(ListChange<&T>)) {
        self.0.changes(since, f)
    }
    pub fn iter(&self) -> IndexIter<&Self> {
        IndexIter::new(self, 0, self.len())
    }
}
impl<T> Index<usize> for ObsListRef<'_, T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("out of index.")
    }
}
impl<'a, T> IntoIterator for &'a ObsListRef<'_, T> {
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
    fn age(&self) -> ObsListAge {
        ObsListAge::Last
    }
    fn len(&self) -> usize {
        self.0.len()
    }
    fn get(&self, index: usize) -> Option<&T> {
        self.0.get(index)
    }
    fn changes(&self, since: &ObsListAge, f: &mut dyn FnMut(ListChange<&T>)) {
        match since {
            ObsListAge::Empty => list_change_for_each(self.0, f),
            ObsListAge::Last => {}
            ObsListAge::Obs(_) => {
                panic!("mismatch source.")
            }
        }
    }
}

struct MapDynObsList<T, F> {
    s: ObsList<T>,
    f: F,
}
struct MapDynObsListRef<'a, T, F> {
    s: ObsListRef<'a, T>,
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
    fn age(&self) -> ObsListAge {
        self.s.age()
    }
    fn len(&self) -> usize {
        self.s.len()
    }
    fn get(&self, index: usize) -> Option<&U> {
        Some((self.f)(self.s.get(index)?))
    }
    fn changes(&self, since: &ObsListAge, f: &mut dyn FnMut(ListChange<&U>)) {
        let m = &self.f;
        self.s.changes(since, &mut |c: ListChange<&T>| f(c.map(m)))
    }
}

impl<T: 'static> From<ObsListCell<T>> for ObsList<T> {
    fn from(s: ObsListCell<T>) -> Self {
        s.as_dyn()
    }
}
impl<T: 'static> From<Vec<T>> for ObsList<T> {
    fn from(values: Vec<T>) -> Self {
        ObsList::from_vec(values)
    }
}
impl<T: 'static> From<Rc<Vec<T>>> for ObsList<T> {
    fn from(values: Rc<Vec<T>>) -> Self {
        ObsList::from_rc_vec(values)
    }
}
impl<'a, T: 'static> From<&'a Rc<Vec<T>>> for ObsList<T> {
    fn from(values: &'a Rc<Vec<T>>) -> Self {
        ObsList::from_rc_vec(values.clone())
    }
}
