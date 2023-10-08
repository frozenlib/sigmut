use crate::test_utils::code_path::{code, CodePathChecker};

use reactive_fn::{collections::vec::ObsVecCell, core::Runtime, Subscription};

#[test]
fn vec_cell_notify() {
    let mut cp = CodePathChecker::new();
    let mut rt = Runtime::new();

    let vec = ObsVecCell::new();

    let _s = {
        let vec = vec.clone();
        Subscription::new(move |oc| {
            let sum: u64 = vec.session().read(oc).iter().sum();
            code(sum);
        })
    };
    rt.update();
    cp.expect("0");
    cp.verify();

    vec.borrow_mut(&mut rt.ac()).push(1);
    rt.update();
    cp.expect("1");
    cp.verify();

    vec.borrow_mut(&mut rt.ac()).push(2);
    rt.update();
    cp.expect("3");
    cp.verify();
}
