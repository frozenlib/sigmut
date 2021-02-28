// use super::*;
// use crate::hot::*;
// use futures::Future;
// use std::{
//     borrow::Borrow,
//     cell::{Ref, RefCell},
//     iter::once,
//     marker::PhantomData,
//     task::Poll,
// };

// pub fn obs_borrow<S, T>(
//     this: S,
//     borrow: impl for<'a> Fn(&'a S, &mut BindContext) -> Ref<'a, T> + 'static,
// ) -> ObsBorrow<impl ObservableBorrow<Item = T>>
// where
//     T: 'static + ?Sized,
//     S: 'static,
// {
//     struct ObsBorrowFn<S, F> {
//         this: S,
//         borrow: F,
//     }
//     impl<T, S, F> ObservableBorrow for ObsBorrowFn<S, F>
//     where
//         T: 'static + ?Sized,
//         S: 'static,
//         for<'a> F: Fn(&'a S, &mut BindContext) -> Ref<'a, T> + 'static,
//     {
//         type Item = T;
//         fn borrow(&self, cx: &mut BindContext) -> Ref<T> {
//             (self.borrow)(&self.this, cx)
//         }
//     }

//     ObsBorrow(ObsBorrowFn { this, borrow })
// }
// pub fn obs_borrow_constant<T: 'static>(value: T) -> ObsBorrow<impl ObservableBorrow<Item = T>> {
//     obs_borrow(RefCell::new(value), |this, _| this.borrow())
// }

// #[derive(Clone)]
// pub struct ObsBorrow<S>(pub(crate) S);

// impl<S: ObservableBorrow> ObsBorrow<S> {
//     pub fn get(&self, cx: &mut BindContext) -> S::Item
//     where
//         S::Item: Copy,
//     {
//         *self.0.borrow(cx)
//     }
//     pub fn borrow(&self, cx: &mut BindContext) -> Ref<S::Item> {
//         self.0.borrow(cx)
//     }
//     pub fn with<U>(
//         &self,
//         f: impl FnOnce(&S::Item, &mut BindContext) -> U,
//         cx: &mut BindContext,
//     ) -> U {
//         f(&self.borrow(cx), cx)
//     }

//     pub fn head(&self) -> Ref<S::Item> {
//         BindContext::with_no_sink(|cx| self.borrow(cx))
//     }
//     pub fn head_tail(&self) -> (Ref<S::Item>, TailRef<impl ObservableRef<Item = S::Item>>)
//     where
//         S: Clone,
//     {
//         BindScope::with(|scope| self.head_tail_with(scope))
//     }

//     pub fn head_tail_with(
//         &self,
//         scope: &BindScope,
//     ) -> (Ref<S::Item>, TailRef<impl ObservableRef<Item = S::Item>>)
//     where
//         S: Clone,
//     {
//         TailRef::new_borrow(self, scope, |s| s.clone().as_ref())
//     }

//     pub fn as_ref(self) -> ObsRef<Self> {
//         ObsRef(self)
//     }
//     pub fn as_any(self) -> ObsBorrow<DynObsBorrow<S::Item>> {
//         ObsBorrow(self.into_dyn())
//     }
//     pub fn into_dyn(self) -> DynObsBorrow<S::Item> {
//         self.0.into_dyn_obs_borrow()
//     }
//     pub fn map<T>(self, f: impl Fn(&S::Item) -> T + 'static) -> Obs<impl Observable<Item = T>> {
//         obs(move |cx| f(&self.borrow(cx)))
//     }
//     pub fn map_ref<T: ?Sized + 'static>(
//         self,
//         f: impl Fn(&S::Item) -> &T + 'static,
//     ) -> ObsBorrow<impl ObservableBorrow<Item = T>> {
//         obs_borrow(self, move |this, cx| Ref::map(this.borrow(cx), &f))
//     }
//     pub fn map_borrow<B: ?Sized + 'static>(self) -> ObsBorrow<impl ObservableBorrow<Item = B>>
//     where
//         S::Item: Borrow<B>,
//     {
//         struct MapBorrow<S, B>
//         where
//             S: ObservableBorrow,
//             S::Item: Borrow<B>,
//             B: ?Sized + 'static,
//         {
//             source: S,
//             _phantom: PhantomData<fn(&S::Item) -> &B>,
//         }
//         impl<S, B> ObservableBorrow for MapBorrow<S, B>
//         where
//             S: ObservableBorrow,
//             S::Item: Borrow<B>,
//             B: ?Sized + 'static,
//         {
//             type Item = B;

//             fn borrow(&self, cx: &mut BindContext) -> Ref<Self::Item> {
//                 Ref::map(self.source.borrow(cx), |x| x.borrow())
//             }
//             fn into_dyn_obs_borrow(self) -> DynObsBorrow<Self::Item>
//             where
//                 Self: Sized,
//             {
//                 self.source.into_dyn_obs_borrow().map_borrow()
//             }
//         }
//         ObsBorrow(MapBorrow {
//             source: self,
//             _phantom: PhantomData,
//         })
//     }
//     pub fn map_as_ref<T: ?Sized + 'static>(self) -> ObsBorrow<impl ObservableBorrow<Item = T>>
//     where
//         S::Item: AsRef<T>,
//     {
//         struct MapAsRef<S, T>
//         where
//             S: ObservableBorrow,
//             S::Item: AsRef<T>,
//             T: ?Sized + 'static,
//         {
//             source: S,
//             _phantom: PhantomData<fn(&S::Item) -> &T>,
//         }
//         impl<S, T> ObservableBorrow for MapAsRef<S, T>
//         where
//             S: ObservableBorrow,
//             S::Item: AsRef<T>,
//             T: ?Sized + 'static,
//         {
//             type Item = T;

//             fn borrow(&self, cx: &mut BindContext) -> Ref<Self::Item> {
//                 Ref::map(self.source.borrow(cx), |x| x.as_ref())
//             }
//             fn into_dyn_obs_borrow(self) -> DynObsBorrow<Self::Item>
//             where
//                 Self: Sized,
//             {
//                 self.source.into_dyn_obs_borrow().map_as_ref()
//             }
//         }
//         ObsBorrow(MapAsRef {
//             source: self,
//             _phantom: PhantomData,
//         })
//     }

//     pub fn flat_map<U: Observable>(
//         self,
//         f: impl Fn(&S::Item) -> U + 'static,
//     ) -> Obs<impl Observable<Item = U::Item>> {
//         obs(move |cx| f(&self.borrow(cx)).get(cx))
//     }

//     pub fn flatten(self) -> Obs<impl Observable<Item = <S::Item as Observable>::Item>>
//     where
//         S::Item: Observable,
//     {
//         obs(move |cx| self.borrow(cx).get(cx))
//     }

//     pub fn map_async_with<Fut>(
//         self,
//         f: impl Fn(&S::Item) -> Fut + 'static,
//         sp: impl LocalSpawn,
//     ) -> ObsBorrow<impl ObservableBorrow<Item = Poll<Fut::Output>> + Clone>
//     where
//         Fut: Future + 'static,
//     {
//         self.as_ref().map_async_with(f, sp)
//     }
//     pub fn cloned(self) -> Obs<impl Observable<Item = S::Item>>
//     where
//         S::Item: Clone,
//     {
//         self.map(|x| x.clone())
//     }
//     pub fn scan<St: 'static>(
//         self,
//         initial_state: St,
//         f: impl Fn(St, &S::Item) -> St + 'static,
//     ) -> ObsBorrow<impl ObservableBorrow<Item = St> + Clone> {
//         self.as_ref().scan(initial_state, f)
//     }
//     pub fn filter_scan<St: 'static>(
//         self,
//         initial_state: St,
//         predicate: impl Fn(&St, &S::Item) -> bool + 'static,
//         f: impl Fn(St, &S::Item) -> St + 'static,
//     ) -> ObsBorrow<impl ObservableBorrow<Item = St> + Clone> {
//         self.as_ref().filter_scan(initial_state, predicate, f)
//     }

//     pub fn dedup_by(
//         self,
//         eq: impl Fn(&S::Item, &S::Item) -> bool + 'static,
//     ) -> ObsBorrow<impl ObservableBorrow<Item = S::Item> + Clone>
//     where
//         S::Item: Clone,
//     {
//         self.cloned().dedup_by(eq)
//     }
//     pub fn dedup_by_key<K: PartialEq>(
//         self,
//         to_key: impl Fn(&S::Item) -> K + 'static,
//     ) -> ObsBorrow<impl ObservableBorrow<Item = S::Item> + Clone>
//     where
//         S::Item: Clone,
//     {
//         self.cloned().dedup_by_key(to_key)
//     }
//     pub fn dedup(self) -> ObsBorrow<impl ObservableBorrow<Item = S::Item> + Clone>
//     where
//         S::Item: PartialEq + Clone,
//     {
//         self.cloned().dedup()
//     }

//     pub fn fold<St: 'static>(
//         self,
//         initial_state: St,
//         f: impl Fn(St, &S::Item) -> St + 'static,
//     ) -> Fold<St> {
//         self.as_ref().fold(initial_state, f)
//     }
//     pub fn collect_to<E: for<'a> Extend<&'a S::Item> + 'static>(self, e: E) -> Fold<E> {
//         self.fold(e, |mut e, x| {
//             e.extend(once(x));
//             e
//         })
//     }
//     pub fn collect<E: for<'a> Extend<&'a S::Item> + Default + 'static>(self) -> Fold<E> {
//         self.collect_to(Default::default())
//     }
//     pub fn collect_vec(self) -> Fold<Vec<S::Item>>
//     where
//         S::Item: Copy,
//     {
//         self.collect()
//     }

//     pub fn subscribe(self, f: impl FnMut(&S::Item) + 'static) -> Subscription {
//         self.as_ref().subscribe(f)
//     }
//     pub fn subscribe_to<O>(self, o: O) -> impl Subscriber<O>
//     where
//         for<'a> O: Observer<&'a S::Item>,
//     {
//         self.as_ref().subscribe_to(o)
//     }

//     pub fn subscribe_async_with<Fut>(
//         self,
//         f: impl FnMut(&S::Item) -> Fut + 'static,
//         sp: impl LocalSpawn,
//     ) -> Subscription
//     where
//         Fut: Future<Output = ()> + 'static,
//     {
//         self.as_ref().subscribe_async_with(f, sp)
//     }
//     pub fn hot(self) -> ObsBorrow<impl ObservableBorrow<Item = S::Item>> {
//         ObsBorrow(Hot::new(self))
//     }
// }

// impl<S: ObservableBorrow> Observable for ObsBorrow<S>
// where
//     S::Item: Copy,
// {
//     type Item = S::Item;

//     fn get(&self, cx: &mut BindContext) -> Self::Item {
//         ObsBorrow::get(self, cx)
//     }
// }
// impl<S: ObservableBorrow> ObservableBorrow for ObsBorrow<S> {
//     type Item = S::Item;
//     fn borrow(&self, cx: &mut BindContext) -> Ref<Self::Item> {
//         ObsBorrow::borrow(self, cx)
//     }
//     fn into_dyn_obs_borrow(self) -> DynObsBorrow<Self::Item>
//     where
//         Self: Sized,
//     {
//         self.0.into_dyn_obs_borrow()
//     }
// }
// impl<S: ObservableBorrow> ObservableRef for ObsBorrow<S> {
//     type Item = S::Item;
//     fn with<U>(
//         &self,
//         f: impl FnOnce(&Self::Item, &mut BindContext) -> U,
//         cx: &mut BindContext,
//     ) -> U {
//         ObsBorrow::with(self, f, cx)
//     }
//     fn into_dyn_obs_ref(self) -> DynObsRef<Self::Item>
//     where
//         Self: Sized,
//     {
//         self.into_dyn().as_ref()
//     }
// }
