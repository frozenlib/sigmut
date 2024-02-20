use std::{
    alloc::Layout,
    cell::Ref,
    fmt::Debug,
    mem::MaybeUninit,
    ops::Deref,
    ptr::{drop_in_place, NonNull},
};

use bumpalo::Bump;

use crate::ObsContext;

pub struct ObsRef<'a, T: ?Sized>(Data<'a, T>);

impl<'a, T: ?Sized> ObsRef<'a, T> {
    pub fn from_value(value: T, oc: &ObsContext<'a>) -> Self
    where
        T: Sized + 'static,
    {
        Self(match Embedded::new(value) {
            Ok(value) => Data::ValueStatic(value),
            Err(value) => MayBox::alloc(value, oc.bump()).into_data(true),
        })
    }

    pub fn from_value_non_static(value: T, oc: &ObsContext<'a>) -> Self
    where
        T: Sized,
    {
        Self(match Embedded::new(value) {
            Ok(value) => Data::ValueAndOwner {
                is_static: false,
                owner: AllocHandle::none(),
                value: Value::Embedded(value),
            },
            Err(value) => MayBox::alloc(value, oc.bump()).into_data(false),
        })
    }

    pub fn map_ref<U: ?Sized>(
        this: Self,
        f: impl for<'b> FnOnce(&'b T, &mut ObsContext<'b>) -> ObsRef<'b, U>,
        oc: &mut ObsContext<'a>,
    ) -> ObsRef<'a, U> {
        unsafe {
            let (is_static, p) = this.0.pin(oc.bump());
            ObsRef(match f(&*p.as_ptr(), oc).0 {
                Data::ValueAndOwner {
                    is_static: false,
                    value,
                    owner,
                } => Data::ValueAndOwner {
                    is_static,
                    value,
                    owner: p.handle.chain(owner, oc.bump()),
                },
                data @ (Data::ValueAndOwner {
                    is_static: true, ..
                }
                | Data::ValueStatic(_)) => data,
            })
        }
    }

    pub fn map<U: ?Sized>(
        this: Self,
        f: impl FnOnce(&T) -> &U,
        oc: &ObsContext<'a>,
    ) -> ObsRef<'a, U> {
        ObsRef(match this.0 {
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
                let (is_static, p) = data.pin(oc.bump());
                p.map(f).into_data(is_static)
            },
        })
    }

    fn storage(&self) -> &str {
        match &self.0 {
            Data::ValueAndOwner { value, .. } => match value {
                Value::Ref(r) => match r {
                    RawRef::Null => "null",
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
impl<T: ?Sized> std::ops::Deref for ObsRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}
impl<'a, T: ?Sized> From<&'a T> for ObsRef<'a, T> {
    fn from(value: &'a T) -> Self {
        Self(Data::ValueAndOwner {
            is_static: false,
            value: Value::Ref(RawRef::Ref(value)),
            owner: AllocHandle::none(),
        })
    }
}
impl<'a, T: ?Sized> From<Ref<'a, T>> for ObsRef<'a, T> {
    fn from(value: Ref<'a, T>) -> Self {
        Self(Data::ValueAndOwner {
            is_static: false,
            value: Value::Ref(RawRef::RefCell(value)),
            owner: AllocHandle::none(),
        })
    }
}
impl<'a, T: ?Sized + Debug> Debug for ObsRef<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynRefInBump")
            //.field("value", &&**self)
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
    ValueStatic(Embedded<'a, T, 6>),
}

impl<'a, T: ?Sized> Data<'a, T> {
    unsafe fn pin(self, b: &'a Bump) -> (bool, MayBox<'a, T>) {
        match self {
            Data::ValueAndOwner {
                is_static,
                value,
                owner,
            } => {
                let value = match value {
                    Value::Ref(value) => match value {
                        RawRef::Null => panic!("ObsRef is null"),
                        RawRef::Ref(value) => MayBox::new(value, AllocHandle::none()),
                        RawRef::RefCell(value) => MayBox::alloc(value, b).map(|x| &**x),
                    },
                    Value::Embedded(value) => value.into_boxed(b),
                };
                (is_static, value.with_owner(owner, b))
            }
            Data::ValueStatic(value) => (true, value.into_boxed(b)),
        }
    }
}
impl<'a, T: ?Sized> Deref for Data<'a, T> {
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
    Embedded(Embedded<'a, T, 3>),
}

#[derive(Default)]
enum RawRef<'a, T: ?Sized> {
    #[default]
    Null,
    Ref(*const T),
    RefCell(Ref<'a, T>),
}
impl<'a, T: ?Sized> RawRef<'a, T> {
    fn map<U: ?Sized>(this: Self, f: impl FnOnce(&T) -> &U) -> RawRef<'a, U> {
        unsafe {
            match this {
                RawRef::Null => RawRef::Null,
                RawRef::Ref(value) => RawRef::Ref(f(&*value)),
                RawRef::RefCell(value) => RawRef::RefCell(Ref::map(value, f)),
            }
        }
    }
}

impl<'a, T: ?Sized> std::ops::Deref for RawRef<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        match self {
            RawRef::Null => panic!("ObsRef is null"),
            RawRef::Ref(r) => unsafe { &**r },
            RawRef::RefCell(r) => r,
        }
    }
}

struct Embedded<'a, T: ?Sized + 'a, const N: usize> {
    array: [isize; N],
    vtbl: Option<&'a EmbeddedVtbl<T, N>>,
}

impl<T, const N: usize> Embedded<'_, T, N> {
    const fn is_supported() -> bool {
        let layout_isize = Layout::new::<isize>();
        let layout_t = Layout::new::<MaybeUninit<T>>();
        layout_t.size() <= layout_isize.size() * N && layout_isize.size() % layout_t.align() == 0
    }
    pub fn new(value: T) -> Result<Self, T> {
        if !Self::is_supported() {
            return Err(value);
        }
        unsafe {
            let mut array = [0isize; N];
            let (&mut [], &mut [ref mut slot, ..], _) = array.align_to_mut::<MaybeUninit<T>>()
            else {
                return Err(value);
            };
            slot.write(value);
            Ok(Self {
                array,
                vtbl: Some(&EmbeddedVtbl {
                    drop: Self::array_drop,
                    as_ref: Self::array_as_ref,
                    into_box: Self::array_into_box,
                }),
            })
        }
    }
    unsafe fn array_drop(array: &mut [isize; N]) {
        unsafe {
            if let ([], [slot, ..], _) = array.align_to_mut::<MaybeUninit<T>>() {
                slot.assume_init_drop()
            } else {
                unreachable!()
            }
        }
    }
    unsafe fn array_as_ref(array: &[isize; N]) -> &T {
        unsafe {
            if let ([], [slot, ..], _) = array.align_to::<MaybeUninit<T>>() {
                slot.assume_init_ref()
            } else {
                unreachable!()
            }
        }
    }
    unsafe fn array_into_box<'b>(array: &mut [isize; N], b: &'b Bump) -> MayBox<'b, T> {
        MayBox::alloc(Embedded::array_into_inner(array), b)
    }
    unsafe fn array_into_inner(array: &mut [isize; N]) -> T {
        if let ([], [slot, ..], _) = array.align_to_mut::<MaybeUninit<T>>() {
            slot.assume_init_read()
        } else {
            unreachable!()
        }
    }
}
impl<T: ?Sized, const N: usize> Embedded<'_, T, N> {
    pub unsafe fn into_boxed(mut self, b: &Bump) -> MayBox<T> {
        (self.vtbl.take().unwrap().into_box)(&mut self.array, b)
    }
}
impl<T: ?Sized, const N: usize> std::ops::Deref for Embedded<'_, T, N> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { (self.vtbl.as_ref().unwrap().as_ref)(&self.array) }
    }
}

impl<T: ?Sized, const N: usize> Drop for Embedded<'_, T, N> {
    fn drop(&mut self) {
        if let Some(vtbl) = self.vtbl.take() {
            unsafe { (vtbl.drop)(&mut self.array) }
        }
    }
}

struct EmbeddedVtbl<T: ?Sized, const N: usize> {
    drop: unsafe fn(&mut [isize; N]),
    as_ref: unsafe fn(&[isize; N]) -> &T,
    into_box: for<'a> unsafe fn(&mut [isize; N], &'a Bump) -> MayBox<'a, T>,
}

struct MayBox<'a, T: ?Sized + 'a> {
    p: *const T,
    handle: AllocHandle<'a>,
}
impl<'a, T: ?Sized + 'a> MayBox<'a, T> {
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

    unsafe fn map<U: ?Sized>(self, f: impl FnOnce(&T) -> &U) -> MayBox<'a, U> {
        unsafe { MayBox::new(f(&*self.p), self.handle) }
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

impl<'a> Drop for RawAllocHandle<'a> {
    fn drop(&mut self) {
        unsafe {
            drop_in_place(self.0.as_ptr());
        }
    }
}

trait DynAllocHandle {}
impl<T> DynAllocHandle for T {}

pub struct ObsRefBuilder<'a, 'b, T: ?Sized> {
    r: ObsRef<'a, T>,
    oc: &'b mut ObsContext<'a>,
}

impl<'a, 'b, T: ?Sized> ObsRefBuilder<'a, 'b, T> {
    pub fn from_value(value: T, oc: &'b mut ObsContext<'a>) -> ObsRefBuilder<'a, 'b, T>
    where
        T: Sized + 'static,
    {
        ObsRefBuilder {
            r: ObsRef::from_value(value, oc),
            oc,
        }
    }
    pub fn from_value_non_static(value: T, oc: &'b mut ObsContext<'a>) -> ObsRefBuilder<'a, 'b, T>
    where
        T: Sized,
    {
        ObsRefBuilder {
            r: ObsRef::from_value_non_static(value, oc),
            oc,
        }
    }
    pub fn from_ref(value: &'a T, oc: &'b mut ObsContext<'a>) -> ObsRefBuilder<'a, 'b, T> {
        ObsRefBuilder {
            r: ObsRef::from(value),
            oc,
        }
    }
    pub fn from_ref_cell(
        value: Ref<'a, T>,
        oc: &'b mut ObsContext<'a>,
    ) -> ObsRefBuilder<'a, 'b, T> {
        ObsRefBuilder {
            r: ObsRef::from(value),
            oc,
        }
    }

    pub fn map<U: ?Sized>(
        self,
        f: impl for<'c> FnOnce(&'c T) -> &'c U,
    ) -> ObsRefBuilder<'a, 'b, U> {
        ObsRefBuilder {
            r: ObsRef::map(self.r, f, self.oc),
            oc: self.oc,
        }
    }

    pub fn map_ref<U: ?Sized>(
        self,
        f: impl for<'c> FnOnce(&'c T, &mut ObsContext<'c>) -> ObsRef<'c, U>,
    ) -> ObsRefBuilder<'a, 'b, U> {
        ObsRefBuilder {
            r: ObsRef::map_ref(self.r, f, self.oc),
            oc: self.oc,
        }
    }

    pub fn build(self) -> ObsRef<'a, T> {
        self.r
    }
}
