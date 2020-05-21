use reactive_fn::*;
use std::{cell::RefCell, rc::Rc};
pub struct Recorder<T> {
    rc: Rc<RefCell<Vec<T>>>,
    unbind: Subscription,
}

#[deprecated(note = "use to_vec.")]
pub fn record<T>(s: &Re<T>) -> Recorder<T> {
    let rc = Rc::new(RefCell::new(Vec::new()));
    let r = rc.clone();
    let unbind = s.for_each(move |x| {
        r.borrow_mut().push(x);
    });
    Recorder { rc, unbind }
}
impl<T> Recorder<T> {
    pub fn finish(self) -> Vec<T> {
        let Recorder { rc, unbind } = self;
        drop(unbind);
        if let Ok(cell) = Rc::try_unwrap(rc) {
            cell.into_inner()
        } else {
            panic!("for_each not complated.");
        }
    }
}
