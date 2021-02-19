use super::*;

pub type SourceList<T> = DynObsList<T>;

impl<T> From<ObsListCell<T>> for SourceList<T>
where
    T: 'static,
{
    fn from(s: ObsListCell<T>) -> Self {
        s.as_dyn()
    }
}
