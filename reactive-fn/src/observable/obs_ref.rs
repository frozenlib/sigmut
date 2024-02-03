use std::{cell::Ref, ptr::drop_in_place};

use bumpalo::Bump;

pub struct ObsRef<'a, T: ?Sized> {
    value: Value<'a, T>,
    owner: Owner<'a>,
}
impl<'a, T> ObsRef<'a, T> {
    pub fn box_in(b: &'a Bump, value: T) -> Self {
        let value = b.alloc(value);
        unsafe {
            Self {
                owner: Owner::new(value),
                value: Value::Direct(value),
            }
        }
    }
}
impl<'a, T: ?Sized> ObsRef<'a, T> {
    pub fn map_ref<U: ?Sized>(
        this: Self,
        b: &'a Bump,
        f: impl for<'b> FnOnce(&'b T) -> Ref<'b, U>,
    ) -> ObsRef<'a, U> {
        match &this.value {
            Value::Cell(_) => {
                let this = b.alloc(this);
                unsafe {
                    ObsRef {
                        owner: Owner::new(this),
                        value: Value::Cell(f(&*this)),
                    }
                }
            }
            Value::Direct(value) => ObsRef {
                value: Value::Cell(f(value)),
                owner: this.owner,
            },
            Value::Null => panic!("ObsRef is null"),
        }
    }
    pub fn map_value<U>(this: Self, b: &'a Bump, f: impl FnOnce(&T) -> U) -> ObsRef<'a, U> {
        let slot = b.alloc((None, this));
        slot.0 = Some(f(&*slot.1));
        unsafe {
            ObsRef {
                owner: Owner::new(slot),
                value: Value::Direct(slot.0.as_ref().unwrap()),
            }
        }
    }
    pub fn map<U: ?Sized>(this: Self, f: impl FnOnce(&T) -> &U) -> ObsRef<'a, U> {
        ObsRef {
            value: Value::map(this.value, f),
            owner: this.owner,
        }
    }
}
impl<T: ?Sized> std::ops::Deref for ObsRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}
impl<'a, T: ?Sized> From<&'a T> for ObsRef<'a, T> {
    fn from(value: &'a T) -> Self {
        Self {
            value: Value::Direct(value),
            owner: Owner::NONE,
        }
    }
}
impl<'a, T: ?Sized> From<Ref<'a, T>> for ObsRef<'a, T> {
    fn from(value: Ref<'a, T>) -> Self {
        Self {
            value: Value::Cell(value),
            owner: Owner::NONE,
        }
    }
}

#[derive(Default)]
enum Value<'a, T: ?Sized> {
    #[default]
    Null,
    Cell(Ref<'a, T>),
    Direct(&'a T),
}
impl<'a, T: ?Sized> Value<'a, T> {
    fn map<U: ?Sized>(this: Self, f: impl FnOnce(&T) -> &U) -> Value<'a, U> {
        match this {
            Value::Cell(value) => Value::Cell(Ref::map(value, f)),
            Value::Direct(value) => Value::Direct(f(value)),
            Value::Null => Value::Null,
        }
    }
}

impl<'a, T: ?Sized> std::ops::Deref for Value<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        match self {
            Value::Cell(cell) => cell,
            Value::Direct(direct) => direct,
            Value::Null => panic!("ObsRef is null"),
        }
    }
}

#[derive(Default)]
struct Owner<'a>(Option<*mut (dyn Droppable + 'a)>);

impl<'a> Owner<'a> {
    const NONE: Self = Owner(None);

    unsafe fn new(owner: *mut (dyn Droppable + 'a)) -> Self {
        Owner(Some(owner))
    }
}

impl Drop for Owner<'_> {
    fn drop(&mut self) {
        unsafe {
            if let Some(owner) = self.0.take() {
                drop_in_place(owner);
            }
        }
    }
}

trait Droppable {}
impl<T> Droppable for T {}
