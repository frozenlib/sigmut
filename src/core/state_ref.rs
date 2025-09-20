use std::{
    alloc::Layout,
    cell::Ref,
    fmt::Debug,
    marker::PhantomData,
    mem::MaybeUninit,
    ops::Deref,
    ptr::{drop_in_place, NonNull},
};

use bumpalo::Bump;

use crate::{core::SignalContext, utils::into_owned};

#[cfg(test)]
mod tests;

/// Abstracted reference.
///
/// Internally it has the following values and then you can get `&T`.
///
/// | Type                          | Created by                       |
/// | ----------------------------- | -------------------------------- |
/// | `T`                           | [`from_value`](Self::from_value) |
/// | `&T`                          | [`From<&T>`]                     |
/// | `RefCell<T>`                  | [`From<RefCell<T>>`]             |
/// | `(StateRef<U>, Fn(&U) -> &T)` | [`map`](Self::map)               |
/// | self-referential struct       | [`map_ref`](Self::map_ref)       |
///
/// To create a complex `StateRef`, use [`StateRefBuilder`](crate::core::StateRefBuilder).
pub struct StateRef<'a, T: ?Sized + 'a>(Data<'a, T>);

impl<'a, T: ?Sized> StateRef<'a, T> {
    /// Create a `StateRef` from a value with `'static` lifetime.
    ///
    /// This method works more efficiently than [`from_value_non_static`](Self::from_value_non_static).
    pub fn from_value<'s: 'a>(value: T, sc: &SignalContext<'s>) -> Self
    where
        T: Sized + 'static,
    {
        Self(match Embedded::new(value) {
            Ok(value) => Data::ValueStatic(value),
            Err(value) => MaybeBox::alloc(value, sc.bump).into_data(true),
        })
    }

    /// Create a `StateRef` from a value with non-`'static` lifetime.
    pub fn from_value_non_static<'s: 'a>(value: T, sc: &SignalContext<'s>) -> Self
    where
        T: Sized,
    {
        Self(match Embedded::new(value) {
            Ok(value) => Data::ValueAndOwner {
                is_static: false,
                owner: AllocHandle::none(),
                value: Value::Embedded(value),
            },
            Err(value) => MaybeBox::alloc(value, sc.bump).into_data(false),
        })
    }

    /// Maps a `StateRef<T>` to a `StateRef<U>` using a function that returns a new `StateRef<U>`.
    ///
    /// If the `StateRef` returned by `f` references `T`, the resulting `StateRef` from `map_ref` will contain a self-reference.
    ///
    /// The third argument to `f` is unused but necessary to add the `'s0: 'a0` constraint.
    pub fn map_ref<'s: 'a, U: ?Sized>(
        this: Self,
        f: impl for<'a0, 's0> FnOnce(&'a0 T, &mut SignalContext<'s0>, &'a0 &'s0 ()) -> StateRef<'a0, U>,
        sc: &mut SignalContext<'s>,
    ) -> StateRef<'a, U> {
        unsafe {
            let (is_static, p) = this.0.pin(sc.bump);
            StateRef(match f(&*p.as_ptr(), sc, &&()).0 {
                Data::ValueAndOwner {
                    is_static: false,
                    value,
                    owner,
                } => Data::ValueAndOwner {
                    is_static,
                    value,
                    owner: p.handle.chain(owner, sc.bump),
                },
                data @ (Data::ValueAndOwner {
                    is_static: true, ..
                }
                | Data::ValueStatic(_)) => data,
            })
        }
    }

    /// Maps a `StateRef<T>` to a `StateRef<U>` using a function that returns a reference `&U`.
    pub fn map<'s: 'a, U: ?Sized>(
        this: Self,
        f: impl FnOnce(&T) -> &U,
        sc: &SignalContext<'s>,
    ) -> StateRef<'a, U> {
        StateRef(match this.0 {
            Data::ValueAndOwner {
                is_static,
                value: Value::Ref(value),
                owner,
            } => Data::ValueAndOwner {
                is_static,
                value: Value::Ref(RawRef::map(value, f)),
                owner,
            },
            data => unsafe {
                let (is_static, p) = data.pin(sc.bump);
                p.map(f).into_data(is_static)
            },
        })
    }
    pub fn into_owned(self) -> <T as ToOwned>::Owned
    where
        T: ToOwned + 'static,
        T::Owned: 'static,
    {
        unsafe {
            match self.0 {
                Data::ValueStatic(value) => value.into_owned(),
                _ => self.to_owned(),
            }
        }
    }

    fn storage(&self) -> &str {
        match &self.0 {
            Data::ValueAndOwner { value, .. } => match value {
                Value::Ref(r) => match r {
                    RawRef::Ref(_) => "ref",
                    RawRef::RefCell(_) => "ref_cell",
                },
                Value::Embedded(_) => "inline",
            },
            Data::ValueStatic(_) => "inline",
        }
    }
    fn has_owner(&self) -> bool {
        match &self.0 {
            Data::ValueAndOwner { owner, .. } => owner.0.is_some(),
            Data::ValueStatic(_) => false,
        }
    }
    fn is_static(&self) -> bool {
        match &self.0 {
            Data::ValueAndOwner { is_static, .. } => *is_static,
            Data::ValueStatic(_) => true,
        }
    }
}
impl<T: ?Sized> std::ops::Deref for StateRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}
impl<'a, T: ?Sized> From<&'a T> for StateRef<'a, T> {
    fn from(value: &'a T) -> Self {
        Self(Data::ValueAndOwner {
            is_static: false,
            value: Value::Ref(RawRef::Ref(value)),
            owner: AllocHandle::none(),
        })
    }
}
impl<'a, T: ?Sized> From<Ref<'a, T>> for StateRef<'a, T> {
    fn from(value: Ref<'a, T>) -> Self {
        Self(Data::ValueAndOwner {
            is_static: false,
            value: Value::Ref(RawRef::RefCell(value)),
            owner: AllocHandle::none(),
        })
    }
}
impl<T: ?Sized + Debug> Debug for StateRef<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynRefInBump")
            .field("value", &&**self)
            .field("storage", &self.storage())
            .field("has_owner", &self.has_owner())
            .field("is_static", &self.is_static())
            .finish()
    }
}

enum Data<'a, T: ?Sized> {
    ValueAndOwner {
        is_static: bool,
        value: Value<'a, T>,
        owner: AllocHandle<'a>,
    },
    ValueStatic(Embedded<'a, T, 3>),
}

impl<'a, T: ?Sized> Data<'a, T> {
    unsafe fn pin(self, b: &'a Bump) -> (bool, MaybeBox<'a, T>) {
        match self {
            Data::ValueAndOwner {
                is_static,
                value,
                owner,
            } => {
                let value = match value {
                    Value::Ref(value) => match value {
                        RawRef::Ref(value) => MaybeBox::new(value, AllocHandle::none()),
                        RawRef::RefCell(value) => MaybeBox::alloc(value, b).map(|x| &**x),
                    },
                    Value::Embedded(value) => value.into_boxed(b),
                };
                (is_static, value.with_owner(owner, b))
            }
            Data::ValueStatic(value) => (true, value.into_boxed(b)),
        }
    }
}
impl<T: ?Sized> Deref for Data<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Data::ValueAndOwner { value, .. } => match value {
                Value::Ref(value) => value,
                Value::Embedded(value) => value,
            },
            Data::ValueStatic(value) => value,
        }
    }
}

enum Value<'a, T: ?Sized> {
    Ref(RawRef<'a, T>),
    Embedded(Embedded<'a, T, 1>),
}

enum RawRef<'a, T: ?Sized> {
    Ref(*const T),
    RefCell(Ref<'a, T>),
}
impl<'a, T: ?Sized> RawRef<'a, T> {
    fn map<U: ?Sized>(this: Self, f: impl FnOnce(&T) -> &U) -> RawRef<'a, U> {
        unsafe {
            match this {
                RawRef::Ref(value) => RawRef::Ref(f(&*value)),
                RawRef::RefCell(value) => RawRef::RefCell(Ref::map(value, f)),
            }
        }
    }
}

impl<T: ?Sized> std::ops::Deref for RawRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        match self {
            RawRef::Ref(r) => unsafe { &**r },
            RawRef::RefCell(r) => r,
        }
    }
}

type BufElement = u128;
type Buf<const N: usize> = [MaybeUninit<BufElement>; N];

struct Embedded<'a, T: ?Sized + 'a, const N: usize> {
    buf: Buf<N>,
    methods: Option<&'a dyn EmbeddedMethods<T, N>>,
}

impl<T, const N: usize> Embedded<'_, T, N> {
    const fn is_supported() -> bool {
        let layout_elem = Layout::new::<BufElement>();
        let layout_t = Layout::new::<MaybeUninit<T>>();
        layout_t.size() <= layout_elem.size() * N
            && layout_elem.size().is_multiple_of(layout_t.align())
    }
    pub fn new(value: T) -> Result<Self, T> {
        if !Self::is_supported() {
            return Err(value);
        }
        unsafe {
            let mut buf = [MaybeUninit::uninit(); N];
            let (&mut [], &mut [ref mut slot, ..], _) = buf.align_to_mut::<MaybeUninit<T>>() else {
                return Err(value);
            };
            slot.write(value);
            Ok(Self {
                buf,
                methods: Some(&EmbeddedMethodsImpl(PhantomData)),
            })
        }
    }
    unsafe fn buf_into_inner(buf: &mut Buf<N>) -> T {
        if let ([], [slot, ..], _) = buf.align_to_mut::<MaybeUninit<T>>() {
            slot.assume_init_read()
        } else {
            unreachable!()
        }
    }
}
impl<T: ?Sized, const N: usize> Embedded<'_, T, N> {
    pub unsafe fn into_boxed(mut self, b: &Bump) -> MaybeBox<'_, T> {
        self.methods.take().unwrap().buf_into_box(&mut self.buf, b)
    }
    pub unsafe fn into_owned(mut self) -> <T as ToOwned>::Owned
    where
        T: ToOwned + 'static,
        T::Owned: 'static,
    {
        self.methods.take().unwrap().buf_into_owned(&mut self.buf)
    }
}
impl<T: ?Sized, const N: usize> std::ops::Deref for Embedded<'_, T, N> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { self.methods.as_ref().unwrap().buf_as_ref(&self.buf) }
    }
}

impl<T: ?Sized, const N: usize> Drop for Embedded<'_, T, N> {
    fn drop(&mut self) {
        if let Some(methods) = self.methods.take() {
            unsafe { methods.buf_drop(&mut self.buf) }
        }
    }
}

trait EmbeddedMethods<T: ?Sized, const N: usize> {
    unsafe fn buf_drop(&self, buf: &mut Buf<N>);
    unsafe fn buf_as_ref<'a>(&self, buf: &'a Buf<N>) -> &'a T;
    unsafe fn buf_into_box<'a>(&self, buf: &mut Buf<N>, b: &'a Bump) -> MaybeBox<'a, T>;
    unsafe fn buf_into_owned(&self, buf: &mut Buf<N>) -> <T as ToOwned>::Owned
    where
        T: ToOwned + 'static,
        T::Owned: 'static;
}
struct EmbeddedMethodsImpl<T, const N: usize>(PhantomData<fn(&mut Buf<N>) -> &T>);

impl<T, const N: usize> EmbeddedMethods<T, N> for EmbeddedMethodsImpl<T, N> {
    unsafe fn buf_drop(&self, buf: &mut Buf<N>) {
        unsafe {
            if let ([], [slot, ..], _) = buf.align_to_mut::<MaybeUninit<T>>() {
                slot.assume_init_drop()
            } else {
                unreachable!()
            }
        }
    }
    unsafe fn buf_as_ref<'a>(&self, buf: &'a Buf<N>) -> &'a T {
        unsafe {
            if let ([], [slot, ..], _) = buf.align_to::<MaybeUninit<T>>() {
                slot.assume_init_ref()
            } else {
                unreachable!()
            }
        }
    }
    unsafe fn buf_into_box<'b>(&self, buf: &mut Buf<N>, b: &'b Bump) -> MaybeBox<'b, T> {
        MaybeBox::alloc(Embedded::buf_into_inner(buf), b)
    }

    unsafe fn buf_into_owned(&self, buf: &mut Buf<N>) -> <T as ToOwned>::Owned
    where
        T: ToOwned + 'static,
        T::Owned: 'static,
    {
        into_owned::<T>(Embedded::buf_into_inner(buf))
    }
}

struct MaybeBox<'a, T: ?Sized + 'a> {
    p: *const T,
    handle: AllocHandle<'a>,
}
impl<'a, T: ?Sized + 'a> MaybeBox<'a, T> {
    unsafe fn new(p: *const T, handle: AllocHandle<'a>) -> Self {
        Self { p, handle }
    }
    fn alloc(value: T, b: &'a Bump) -> Self
    where
        T: Sized,
    {
        let value = b.alloc(value);
        unsafe { Self::new(value, AllocHandle::new(value)) }
    }
    fn as_ptr(&self) -> *const T {
        self.p
    }

    unsafe fn map<U: ?Sized>(self, f: impl FnOnce(&T) -> &U) -> MaybeBox<'a, U> {
        unsafe { MaybeBox::new(f(&*self.p), self.handle) }
    }
    fn with_owner<'b: 'a>(self, owner: AllocHandle<'b>, b: &'a Bump) -> Self {
        Self {
            p: self.p,
            handle: owner.chain(self.handle, b),
        }
    }
    fn into_data(self, is_static: bool) -> Data<'a, T> {
        Data::ValueAndOwner {
            is_static,
            value: Value::Ref(RawRef::Ref(self.p)),
            owner: self.handle,
        }
    }
}

struct AllocHandle<'a>(Option<RawAllocHandle<'a>>);

impl<'a> AllocHandle<'a> {
    fn none() -> Self {
        Self(None)
    }

    unsafe fn new(p: *mut (impl DynAllocHandle + 'a)) -> Self {
        let p: *mut dyn DynAllocHandle = p;
        Self(NonNull::new(p).map(RawAllocHandle))
    }

    fn chain<'b>(self, value: AllocHandle<'b>, b: &'b Bump) -> AllocHandle<'b>
    where
        'a: 'b,
    {
        AllocHandle(match (self.0, value.0) {
            (None, None) => None,
            (None, Some(value)) => Some(value),
            (Some(owner), None) => Some(owner),
            (Some(owner), Some(value)) => unsafe { AllocHandle::new(b.alloc((value, owner))).0 },
        })
    }
}

struct RawAllocHandle<'a>(NonNull<dyn DynAllocHandle + 'a>);

impl Drop for RawAllocHandle<'_> {
    fn drop(&mut self) {
        unsafe {
            drop_in_place(self.0.as_ptr());
        }
    }
}

trait DynAllocHandle {}
impl<T> DynAllocHandle for T {}
