use super::*;
use std::rc::Rc;

// pub trait DynamicObservable: 'static {
//     type Item;
//     fn dyn_get(&self, cx: &mut BindContext) -> Self::Item;
//     fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = Self::Item>>;
// }

// pub trait DynamicObservableSource: 'static {
//     type Item;
//     fn dyn_get(self: Rc<Self>, cx: &mut BindContext) -> Self::Item;
//     fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRefSource<Item = Self::Item>>;
// }

// pub trait DynamicObservableBorrow: 'static {
//     type Item: ?Sized;
//     fn dyn_borrow(&self, cx: &mut BindContext) -> Ref<Self::Item>;
//     fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = Self::Item>>;
// }
// pub trait DynamicObservableBorrowSource: Any + 'static {
//     type Item: ?Sized;

//     fn dyn_borrow(
//         &self,
//         rc_self: &Rc<dyn DynamicObservableBorrowSource<Item = Self::Item>>,
//         cx: &mut BindContext,
//     ) -> Ref<Self::Item>;
//     fn as_rc_any(self: Rc<Self>) -> Rc<dyn Any>;

//     fn downcast(rc_self: &Rc<dyn DynamicObservableBorrowSource<Item = Self::Item>>) -> Rc<Self>
//     where
//         Self: Sized,
//     {
//         rc_self.clone().as_rc_any().downcast::<Self>().unwrap()
//     }

//     fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRefSource<Item = Self::Item>>;
// }

pub trait DynamicObservableRef: 'static {
    type Item: ?Sized;
    fn dyn_with(&self, f: &mut dyn FnMut(&Self::Item, &mut BindContext), cx: &mut BindContext);
}
pub trait DynamicObservableRefSource: 'static {
    type Item: ?Sized;
    fn dyn_with(
        self: Rc<Self>,
        f: &mut dyn FnMut(&Self::Item, &mut BindContext),
        cx: &mut BindContext,
    );
}
pub struct DynamicObs<S>(pub S);
// impl<T, S: Observable<Item = T> + ObservableRef<Item = T>> DynamicObservable for DynamicObs<S> {
//     type Item = T;
//     fn dyn_get(&self, cx: &mut BindContext) -> T {
//         self.0.get(cx)
//     }
//     fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = T>> {
//         self
//     }
// }
// impl<T: ?Sized, S: ObservableBorrow<Item = T> + ObservableRef<Item = T>> DynamicObservableBorrow
//     for DynamicObs<S>
// {
//     type Item = T;
//     fn dyn_borrow(&self, cx: &mut BindContext) -> Ref<T> {
//         self.0.borrow(cx)
//     }
//     fn as_ref(self: Rc<Self>) -> Rc<dyn DynamicObservableRef<Item = T>> {
//         self
//     }
// }

impl<T: ?Sized, S: ObservableRef<Item = T>> DynamicObservableRef for DynamicObs<S> {
    type Item = T;
    fn dyn_with(&self, f: &mut dyn FnMut(&T, &mut BindContext), cx: &mut BindContext) {
        self.0.with(f, cx)
    }
}
// impl<S: Observable> Observable for DynamicObs<S> {
//     type Item = S::Item;
//     fn get(&self, cx: &mut BindContext) -> Self::Item {
//         self.0.get(cx)
//     }
// }
// impl<S: ObservableBorrow> ObservableBorrow for DynamicObs<S> {
//     type Item = S::Item;
//     fn borrow(&self, cx: &mut BindContext) -> Ref<Self::Item> {
//         self.0.borrow(cx)
//     }
// }
impl<S: ObservableRef> ObservableRef for DynamicObs<S> {
    type Item = S::Item;
    fn with<U>(
        &self,
        f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
        cx: &mut BindContext,
    ) -> U {
        self.0.with(f, cx)
    }
}
