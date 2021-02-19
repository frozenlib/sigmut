pub struct ObsListIter<S> {
    s: S,
    len: usize,
}
impl<S: Index<usize>> Iterator for ObsListIter<S> {}
