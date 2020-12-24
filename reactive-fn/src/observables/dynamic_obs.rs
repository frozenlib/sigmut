use crate::*;
use std::{any::Any, cell::Ref, rc::Rc};

pub trait DynamicObservable: 'static {
    type Item;
    fn dyn_get(&self, cx: &BindContext) -> Self::Item;
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = Self::Item>>;
}

pub trait DynamicObservableSource: 'static {
    type Item;
    fn dyn_get(self: Rc<Self>, cx: &BindContext) -> Self::Item;
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRefSource<Item = Self::Item>>;
}

pub trait DynamicObservableBorrow: 'static {
    type Item: ?Sized;
    fn dyn_borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item>;
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = Self::Item>>;
}
pub trait DynamicObservableBorrowSource: Any + 'static {
    type Item: ?Sized;

    fn dyn_borrow<'a>(
        &'a self,
        rc_self: &Rc<dyn DynamicObservableBorrowSource<Item = Self::Item>>,
        cx: &BindContext<'a>,
    ) -> Ref<'a, Self::Item>;
    fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any>;

    fn downcast(rc_self: &Rc<dyn DynamicObservableBorrowSource<Item = Self::Item>>) -> Rc<Self>
    where
        Self: Sized,
    {
        rc_self.clone().as_rc_any().downcast::<Self>().unwrap()
    }

    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRefSource<Item = Self::Item>>;
}

pub trait DynamicObservableRef: 'static {
    type Item: ?Sized;
    fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext);
    fn copied(self: Rc<Self>) -> Rc<dyn DynamicObservable<Item = Self::Item>>
    where
        Self::Item: Copy;
}
pub trait DynamicObservableRefSource: 'static {
    type Item: ?Sized;
    fn dyn_with(self: Rc<Self>, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext);
    // fn copied(self: Rc<Self>) -> Rc<dyn DynamicObservableSource<Item = Self::Item>>
    // where
    //     Self::Item: Copy;
}
pub struct DynamicObs<S>(pub S);
impl<S: Observable> DynamicObservable for DynamicObs<S> {
    type Item = S::Item;
    fn dyn_get(&self, cx: &BindContext) -> Self::Item {
        self.0.get(cx)
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = Self::Item>> {
        self
    }
}
impl<S: Observable> DynamicObservableRef for DynamicObs<S> {
    type Item = S::Item;
    fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
        f(&self.0.get(cx), cx)
    }
    fn copied(self: Rc<Self>) -> Rc<dyn DynamicObservable<Item = Self::Item>>
    where
        Self::Item: Copy,
    {
        self
    }
}

pub struct DynamicObsBorrow<S>(pub S);
impl<S: ObservableBorrow> DynamicObservable for DynamicObsBorrow<S>
where
    S::Item: Copy,
{
    type Item = S::Item;

    fn dyn_get(&self, cx: &BindContext) -> Self::Item {
        *self.0.borrow(cx)
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = Self::Item>> {
        self
    }
}
impl<S: ObservableBorrow> DynamicObservableBorrow for DynamicObsBorrow<S> {
    type Item = S::Item;
    fn dyn_borrow<'a>(&'a self, cx: &BindContext<'a>) -> Ref<'a, Self::Item> {
        self.0.borrow(cx)
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = Self::Item>> {
        self
    }
}
impl<S: ObservableBorrow> DynamicObservableRef for DynamicObsBorrow<S> {
    type Item = S::Item;
    fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
        f(&self.0.borrow(cx), cx)
    }
    fn copied(self: Rc<Self>) -> Rc<dyn DynamicObservable<Item = Self::Item>>
    where
        Self::Item: Copy,
    {
        self
    }
}

pub struct DynamicObsRef<S>(pub S);
impl<S: ObservableRef> DynamicObservable for DynamicObsRef<S>
where
    S::Item: Copy,
{
    type Item = S::Item;
    fn dyn_get(&self, cx: &BindContext) -> Self::Item {
        self.0.with(|value, _| *value, cx)
    }
    fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = Self::Item>> {
        self
    }
}
impl<S: ObservableRef> DynamicObservableRef for DynamicObsRef<S> {
    type Item = S::Item;
    fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &BindContext), cx: &BindContext) {
        self.0.with(f, cx)
    }

    fn copied(self: Rc<Self>) -> Rc<dyn DynamicObservable<Item = Self::Item>>
    where
        Self::Item: Copy,
    {
        self
    }
}
