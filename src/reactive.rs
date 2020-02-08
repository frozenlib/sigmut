// pub mod cell;
// mod map_async;

// pub trait Re {
//     type Item;
//     fn get(&self, ctx: &mut BindContext) -> Self::Item;

//     fn for_each<F: Fn(Self::Item) + 'static>(self, f: F) -> Subscription
//     where
//         Self: Sized + 'static,
//     {
//         Subscription(ForEachData::new(self, f))
//     }

//     fn map<F: Fn(Self::Item) -> U, U>(self, f: F) -> Map<Self, F>
//     where
//         Self: Sized,
//     {
//         Map { s: self, f }
//     }
//     fn flat_map<F: Fn(Self::Item) -> R, R: Re>(self, f: F) -> FlatMap<Self, F>
//     where
//         Self: Sized,
//     {
//         FlatMap { s: self, f }
//     }

//     fn map_async_cached<F, Fut, T, GetPendingValue, Spawn>(
//         self,
//         f: F,
//         get_pending_value: GetPendingValue,
//         spawn: Spawn,
//     ) -> RcReRef<T>
//     where
//         Self: Sized + 'static,
//         F: Fn(Self::Item) -> Fut + 'static,
//         Fut: std::future::Future<Output = T> + 'static,
//         T: 'static,
//         GetPendingValue: Fn(Option<T>) -> T + 'static,
//         Spawn: futures::task::LocalSpawn + 'static,
//     {
//         RcReRef(Rc::new(map_async::MapAsyncCacheData::new(
//             self,
//             f,
//             get_pending_value,
//             spawn,
//         )))
//     }

//     fn cached(self) -> RcReRef<Self::Item>
//     where
//         Self: Sized + 'static,
//     {
//         RcReRef(Rc::new(CacheData::new(self)))
//     }

//     fn rc(self) -> RcRe<Self::Item>
//     where
//         Self: Sized + 'static,
//     {
//         RcRe(Rc::new(self))
//     }
// }
// pub trait ReRef {
//     type Item;
//     fn borrow(&self, ctx: &mut BindContext) -> Ref<Self::Item>;

//     fn for_each<F: Fn(&Self::Item) + 'static>(self, f: F) -> Subscription
//     where
//         Self: Sized + 'static,
//     {
//         Subscription(RefForEachData::new(self, f))
//     }

//     fn map<F: Fn(&Self::Item) -> U, U>(self, f: F) -> RefMap<Self, F>
//     where
//         Self: Sized,
//     {
//         RefMap { s: self, f }
//     }
//     fn map_ref<F: Fn(&Self::Item) -> &U, U>(self, f: F) -> RefMapRef<Self, F>
//     where
//         Self: Sized,
//     {
//         RefMapRef { s: self, f }
//     }
//     fn flat_map<F: Fn(&Self::Item) -> U, U: Re>(self, f: F) -> RefFlatMap<Self, F>
//     where
//         Self: Sized,
//     {
//         RefFlatMap { s: self, f }
//     }

//     fn cloned(self) -> Cloned<Self>
//     where
//         Self: Sized,
//         Self::Item: Clone,
//     {
//         Cloned(self)
//     }

//     fn rc(self) -> RcReRef<Self::Item>
//     where
//         Self: Sized + 'static,
//     {
//         RcReRef(Rc::new(self))
//     }
// }

// trait DynRe<T> {
//     fn dyn_get(self: Rc<Self>, ctx: &mut BindContext) -> T;
// }

// // When arbitrary_self_types is stabilized,
// // change to `fn dyn_borrow(self: &Rc<Self>, ctx: &mut BindContext) -> Ref<T>`;
// trait DynReRef<T> {
//     fn dyn_borrow(&self, this: &dyn Any, ctx: &mut BindContext) -> Ref<T>;
//     fn downcast(this: &dyn Any) -> &Rc<Self>
//     where
//         Self: Sized + 'static,
//     {
//         this.downcast_ref().unwrap()
//     }
// }

// impl<R: Re> DynRe<R::Item> for R {
//     fn dyn_get(self: Rc<Self>, ctx: &mut BindContext) -> R::Item {
//         self.get(ctx)
//     }
// }
// impl<R: ReRef + Any> DynReRef<R::Item> for R {
//     fn dyn_borrow(&self, _this: &dyn Any, ctx: &mut BindContext) -> Ref<R::Item> {
//         self.borrow(ctx)
//     }
// }

// #[derive(Clone)]
// pub struct RcRe<T>(Rc<dyn DynRe<T>>);

// #[derive(Clone)]
// pub struct RcReRef<T>(Rc<dyn DynReRef<T>>);

// impl<T> RcRe<T> {}
// impl<T> RcReRef<T> {}

// impl<T> Re for RcRe<T> {
//     type Item = T;
//     fn get(&self, ctx: &mut BindContext) -> T {
//         self.0.clone().dyn_get(ctx)
//     }
//     fn rc(self) -> RcRe<T> {
//         self
//     }
// }
// impl<T: 'static> ReRef for RcReRef<T> {
//     type Item = T;

//     fn borrow(&self, ctx: &mut BindContext) -> Ref<T> {
//         self.0.dyn_borrow(&self.0, ctx)
//     }
//     fn rc(self) -> RcReRef<T> {
//         self
//     }
// }

// impl<T, F: Fn(&BindContext) -> T> Re for F {
//     type Item = T;
//     fn get(&self, ctx: &mut BindContext) -> T {
//         self(ctx)
//     }
// }
// impl<S: Re> Re for Option<S> {
//     type Item = Option<S::Item>;
//     fn get(&self, ctx: &mut BindContext) -> Self::Item {
//         self.as_ref().map(|s| s.get(ctx))
//     }
// }
// impl<S0: Re, S1: Re> Re for (S0, S1) {
//     type Item = (S0::Item, S1::Item);
//     fn get(&self, ctx: &mut BindContext) -> Self::Item {
//         (self.0.get(ctx), self.1.get(ctx))
//     }
// }

// struct CacheData<S: Re> {
//     src: S,
//     value: RefCell<Option<S::Item>>,
//     sinks: BindSinks,
//     binds: RefCell<Bindings>,
// }
// impl<S: Re> CacheData<S> {
//     fn new(src: S) -> Self {
//         CacheData {
//             src,
//             value: RefCell::new(None),
//             sinks: BindSinks::new(),
//             binds: RefCell::new(Bindings::new()),
//         }
//     }
// }
// impl<S: Re> BindSource for CacheData<S> {
//     fn sinks(&self) -> &BindSinks {
//         &self.sinks
//     }
// }
// impl<S: Re> BindSink for CacheData<S> {
//     fn lock(&self) {
//         self.sinks.lock();
//     }
//     fn unlock(self: Rc<Self>, modified: bool) {
//         self.sinks.unlock_with(modified, || {
//             self.value.replace(None);
//         });
//     }
// }

// impl<S: Re + 'static> DynReRef<S::Item> for CacheData<S> {
//     fn dyn_borrow(&self, this: &dyn Any, ctx: &mut BindContext) -> Ref<S::Item> {
//         let this = Self::downcast(this);
//         ctx.bind(this.clone());
//         let mut b = self.value.borrow();
//         if b.is_none() {
//             drop(b);
//             *self.value.borrow_mut() = Some(
//                 self.src
//                     .get(&mut self.binds.borrow_mut().context(this.clone())),
//             );
//             b = self.value.borrow();
//         }
//         return Ref::Cell(std::cell::Ref::map(b, |x| x.as_ref().unwrap()));
//     }
// }

// pub struct Constant<T>(T);
// impl<T> ReRef for Constant<T> {
//     type Item = T;
//     fn borrow(&self, _ctx: &mut BindContext) -> Ref<Self::Item> {
//         Ref::Native(&self.0)
//     }
// }

// pub struct Cloned<S>(S);
// impl<S: ReRef> Re for Cloned<S>
// where
//     S::Item: Clone,
// {
//     type Item = S::Item;
//     fn get(&self, ctx: &mut BindContext) -> Self::Item {
//         self.0.borrow(ctx).clone()
//     }
// }

// pub struct Map<S, F> {
//     s: S,
//     f: F,
// }

// impl<S: Re, F: Fn(S::Item) -> U, U> Re for Map<S, F> {
//     type Item = U;
//     fn get(&self, ctx: &mut BindContext) -> Self::Item {
//         (self.f)(self.s.get(ctx))
//     }
// }

// pub struct RefMap<S, F> {
//     s: S,
//     f: F,
// }
// impl<S: ReRef, F: Fn(&S::Item) -> U, U> Re for RefMap<S, F> {
//     type Item = U;
//     fn get(&self, ctx: &mut BindContext) -> Self::Item {
//         (self.f)(&self.s.borrow(ctx))
//     }
// }

// pub struct RefMapRef<S, F> {
//     s: S,
//     f: F,
// }

// impl<S: ReRef, F: Fn(&S::Item) -> &U, U> ReRef for RefMapRef<S, F> {
//     type Item = U;
//     fn borrow(&self, ctx: &mut BindContext) -> Ref<U> {
//         Ref::map(self.s.borrow(ctx), &self.f)
//     }
// }

// pub struct FlatMap<S, F> {
//     s: S,
//     f: F,
// }

// impl<S: Re, F: Fn(S::Item) -> U, U: Re> Re for FlatMap<S, F> {
//     type Item = U::Item;
//     fn get(&self, ctx: &mut BindContext) -> Self::Item {
//         (self.f)(self.s.get(ctx)).get(ctx)
//     }
// }

// pub struct RefFlatMap<S, F> {
//     s: S,
//     f: F,
// }

// impl<S: ReRef, F: Fn(&S::Item) -> U, U: Re> Re for RefFlatMap<S, F> {
//     type Item = U::Item;
//     fn get(&self, ctx: &mut BindContext) -> Self::Item {
//         (self.f)(&self.s.borrow(ctx)).get(ctx)
//     }
// }

// pub struct Subscription(Rc<dyn BindSink>);

// struct ForEachData<S, F> {
//     s: S,
//     f: F,
//     binds: RefCell<Bindings>,
//     ls: RefCell<LockState>,
// }

// impl<S: Re + 'static, F: Fn(S::Item) + 'static> ForEachData<S, F> {
//     fn new(s: S, f: F) -> Rc<Self> {
//         let this = Rc::new(Self {
//             s,
//             f,
//             binds: RefCell::new(Bindings::new()),
//             ls: RefCell::new(LockState::new()),
//         });
//         Self::run(&this);
//         this
//     }
//     fn run(this: &Rc<Self>) {
//         let sink = this.clone();
//         (this.f)(this.s.get(&mut this.binds.borrow_mut().context(sink)));
//     }
// }

// impl<S: Re + 'static, F: Fn(S::Item) + 'static> BindSink for ForEachData<S, F> {
//     fn lock(&self) {
//         self.ls.borrow_mut().lock();
//     }
//     fn unlock(self: Rc<Self>, modified: bool) {
//         let mut ls = self.ls.borrow_mut();
//         if ls.unlock(modified) {
//             drop(ls);
//             ForEachData::run(&self);
//         }
//     }
// }

// struct RefForEachData<S, F> {
//     s: S,
//     f: F,
//     binds: RefCell<Bindings>,
//     ls: RefCell<LockState>,
// }

// impl<S: ReRef + 'static, F: Fn(&S::Item) + 'static> RefForEachData<S, F> {
//     fn new(s: S, f: F) -> Rc<Self> {
//         let this = Rc::new(Self {
//             s,
//             f,
//             binds: RefCell::new(Bindings::new()),
//             ls: RefCell::new(LockState::new()),
//         });
//         Self::run(&this);
//         this
//     }
//     fn run(this: &Rc<Self>) {
//         let sink = this.clone();
//         (this.f)(&this.s.borrow(&mut this.binds.borrow_mut().context(sink)));
//     }
// }

// impl<S: ReRef + 'static, F: Fn(&S::Item) + 'static> BindSink for RefForEachData<S, F> {
//     fn lock(&self) {
//         self.ls.borrow_mut().lock();
//     }
//     fn unlock(self: Rc<Self>, modified: bool) {
//         let mut ls = self.ls.borrow_mut();
//         if ls.unlock(modified) {
//             drop(ls);
//             RefForEachData::run(&self);
//         }
//     }
// }
