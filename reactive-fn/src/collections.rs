pub mod obs_list;
pub mod source_list;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum ListChangeKind {
    Insert,
    Remove,
    Modify,
}

pub struct ListChange<'a, T> {
    pub kind: ListChangeKind,

    /// Index of the changed element. (The index at the time the change was made.)
    pub index: usize,

    /// The most recent value, not the one immediately after it was changed.
    pub value: &'a T,
}
