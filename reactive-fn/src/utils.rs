use std::{
    any::{Any, TypeId},
    mem::swap,
    ops::{Deref, DerefMut},
};
#[inline]
pub fn cast_or_convert<T: 'static, M: 'static>(value: T, convert: impl FnOnce(T) -> M) -> M {
    if TypeId::of::<T>() == TypeId::of::<M>() {
        cast(value)
    } else {
        convert(value)
    }
}
fn cast<T: 'static, M: 'static>(value: T) -> M {
    let mut value_any = Some(value);
    let mut value_typed: Option<M> = None;
    swap(
        <dyn Any>::downcast_mut(&mut value_any).unwrap(),
        &mut value_typed,
    );
    value_typed.unwrap()
}

pub(crate) struct SafeManuallyDrop<T>(Option<T>);

impl<T> SafeManuallyDrop<T> {
    pub fn new(value: T) -> Self {
        Self(Some(value))
    }
    pub fn drop(this: &mut Self) {
        this.0.take();
    }
}
impl<T> Deref for SafeManuallyDrop<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().expect("already dropped")
    }
}
impl<T> DerefMut for SafeManuallyDrop<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut().expect("already dropped")
    }
}
