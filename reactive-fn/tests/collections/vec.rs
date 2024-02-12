use assert_call::{call, CallRecorder};
use reactive_fn::{collections::vec::ObsVecCell, core::Runtime, Subscription};

#[test]
fn vec_cell_notify() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let vec = ObsVecCell::new();

    let _s = {
        let vec = vec.clone();
        Subscription::new(move |oc| {
            let sum: u64 = vec.session().read(oc).iter().sum();
            call!("{sum}");
        })
    };
    rt.update();
    c.verify("0");

    vec.borrow_mut(&mut rt.ac()).push(1);
    rt.update();
    c.verify("1");

    vec.borrow_mut(&mut rt.ac()).push(2);
    rt.update();
    c.verify("3");
}
