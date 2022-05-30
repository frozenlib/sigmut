use std::{
    any::{Any, TypeId},
    mem::swap,
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
