use std::any::Any;
use std::marker::PhantomData;

#[derive(Default)]
pub(crate) struct PhantomNotSend(PhantomData<*mut u8>);

pub(crate) fn downcast<T: 'static, S: 'static>(value: S) -> Result<T, S> {
    let mut value = Some(value);
    if let Some(value) = <dyn Any>::downcast_mut::<Option<T>>(&mut value) {
        Ok(value.take().unwrap())
    } else {
        Err(value.unwrap())
    }
}
// pub(crate) fn downcast_this<'a, T: 'static>(_: &T, this: &'a dyn Any) -> &'a T {
//     this.downcast_ref().unwrap()
// }

#[allow(clippy::redundant_clone)]
pub(crate) fn into_owned<T>(value: T) -> T::Owned
where
    T: ToOwned + 'static,
    T::Owned: 'static,
{
    match downcast::<T::Owned, _>(value) {
        Ok(value) => value,
        Err(value) => value.to_owned(),
    }
}

// pub(crate) fn get_or_init<T>(cell: &RefCell<Option<T>>, init: impl FnOnce() -> T) -> Ref<T> {
//     if cell.borrow().is_none() {
//         *cell.borrow_mut() = Some(init());
//     }
//     Ref::map(cell.borrow(), |x| x.as_ref().unwrap())
// }

// pub(crate) struct IdPool {
//     free_ids: Vec<usize>,
//     next_id: usize,
// }

// impl IdPool {
//     pub fn new() -> Self {
//         Self {
//             free_ids: Vec::new(),
//             next_id: 0,
//         }
//     }
//     pub fn len(&self) -> usize {
//         self.next_id - self.free_ids.len()
//     }
//     pub fn end(&self) -> usize {
//         self.next_id
//     }
//     pub fn is_empty(&self) -> bool {
//         self.len() == 0
//     }

//     pub fn alloc(&mut self) -> usize {
//         if let Some(id) = self.free_ids.pop() {
//             id
//         } else {
//             let id = self.next_id;
//             self.next_id += 1;
//             id
//         }
//     }
//     pub fn free(&mut self, id: usize) {
//         if id == self.next_id - 1 {
//             self.next_id -= 1;
//         } else {
//             self.free_ids.push(id);
//         }
//     }
// }
