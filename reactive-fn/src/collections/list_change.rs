#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum ListChangeKind {
    Insert,
    Remove,
    Modify,
}

pub struct ListChange<T> {
    pub kind: ListChangeKind,

    /// Index of the changed element. (The index at the time the change was made.)
    pub index: usize,

    /// The most recent value, not the one immediately after it was changed.
    pub value: T,
}
impl<T> ListChange<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> ListChange<U> {
        ListChange {
            kind: self.kind,
            index: self.index,
            value: f(self.value),
        }
    }
}
pub(crate) fn list_change_for_each<'a, T: 'a>(
    values: impl IntoIterator<Item = &'a T>,
    mut f: impl FnMut(ListChange<&T>),
) {
    for (index, value) in values.into_iter().enumerate() {
        f(ListChange {
            index,
            value,
            kind: ListChangeKind::Insert,
        })
    }
}
