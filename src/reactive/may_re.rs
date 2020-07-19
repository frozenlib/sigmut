use super::*;

#[derive(Clone)]
pub struct MayRe<T: 'static>(MayReData<T>);

#[derive(Clone)]
enum MayReData<T: 'static> {
    Value(T),
    Re(Re<T>),
}

impl<T> Clone for MayReRef<T>
where
    T: Clone + ?Sized + 'static,
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: 'static> From<T> for MayRe<T> {
    fn from(value: T) -> Self {
        MayRe(MayReData::Value(value))
    }
}

impl<T: Copy + 'static> From<&T> for MayRe<T> {
    fn from(value: &T) -> Self {
        MayRe(MayReData::Value(*value))
    }
}
impl<T: 'static> From<Re<T>> for MayRe<T> {
    fn from(source: Re<T>) -> Self {
        MayRe(MayReData::Re(source))
    }
}
impl<T: 'static> From<&Re<T>> for MayRe<T> {
    fn from(source: &Re<T>) -> Self {
        MayRe(MayReData::Re(source.clone()))
    }
}
impl<T: Copy + 'static> From<ReRef<T>> for MayRe<T> {
    fn from(source: ReRef<T>) -> Self {
        MayRe(MayReData::Re(source.cloned()))
    }
}
impl<T: Copy + 'static> From<&ReRef<T>> for MayRe<T> {
    fn from(source: &ReRef<T>) -> Self {
        MayRe(MayReData::Re(source.cloned()))
    }
}
impl<T: Copy + 'static> From<ReBorrow<T>> for MayRe<T> {
    fn from(source: ReBorrow<T>) -> Self {
        MayRe(MayReData::Re(source.cloned()))
    }
}
impl<T: Copy + 'static> From<&ReBorrow<T>> for MayRe<T> {
    fn from(source: &ReBorrow<T>) -> Self {
        MayRe(MayReData::Re(source.cloned()))
    }
}
