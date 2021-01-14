pub trait Observer<T>: 'static {
    fn next(&mut self, value: T);
    fn into_dyn(self) -> DynObserver<T>
    where
        Self: Sized,
    {
        DynObserver(Some(Box::new(self)))
    }
}
impl<T, F: FnMut(T) -> () + 'static> Observer<T> for F {
    fn next(&mut self, value: T) {
        self(value)
    }
}

pub struct DynObserver<T>(Option<Box<dyn Observer<T>>>);

impl<T> DynObserver<T> {
    pub fn null() -> Self {
        Self(None)
    }
    pub fn is_null(&self) -> bool {
        self.0.is_none()
    }
}

impl<T: 'static> Observer<T> for DynObserver<T> {
    fn next(&mut self, value: T) {
        if let Some(o) = &mut self.0 {
            o.next(value);
        }
    }
    fn into_dyn(self) -> DynObserver<T> {
        self
    }
}