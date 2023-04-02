use crate::test_utils::code_path::{code, CodePathChecker};
use futures::channel::oneshot::channel;
use reactive_fn::core::Runtime;
use reactive_fn::{Obs, ObsCell};
use std::task::Poll;
use std::{cell::RefCell, rc::Rc};

#[test]
fn new() {
    let mut cp = CodePathChecker::new();
    let mut dc = Runtime::new();
    let cell0 = ObsCell::new(1);
    let cell1 = ObsCell::new(10);
    let s = Obs::new({
        let cell0 = cell0.obs();
        let cell1 = cell1.obs();
        move |oc| {
            code("call");
            cell0.get(oc) + cell1.get(oc)
        }
    });
    let ac = &mut dc.ac();

    assert_eq!(s.get(ac.oc()), 11);
    cp.expect("call");
    cp.verify();
    assert_eq!(s.get(ac.oc()), 11);
    cp.verify();

    cell0.set(2, ac);
    assert_eq!(s.get(ac.oc()), 12);
    cp.expect("call");
    cp.verify();
    assert_eq!(s.get(ac.oc()), 12);
    cp.verify();

    cell0.set(3, ac);
    cell1.set(30, ac);
    assert_eq!(s.get(ac.oc()), 33);
    cp.expect("call");
    cp.verify();
    assert_eq!(s.get(ac.oc()), 33);
    cp.verify();
}

#[test]
fn from_get() {
    let mut cp = CodePathChecker::new();
    let mut dc = Runtime::new();
    let cell0 = ObsCell::new(1);
    let cell1 = ObsCell::new(10);
    let s = Obs::from_get({
        let cell0 = cell0.obs();
        let cell1 = cell1.obs();
        move |oc| {
            code("call");
            cell0.get(oc) + cell1.get(oc)
        }
    });
    let ac = &mut dc.ac();

    assert_eq!(s.get(ac.oc()), 11);
    cp.expect("call");
    cp.verify();
    assert_eq!(s.get(ac.oc()), 11);
    cp.expect("call");
    cp.verify();

    cell0.set(2, ac);
    assert_eq!(s.get(ac.oc()), 12);
    cp.expect("call");
    cp.verify();
    assert_eq!(s.get(ac.oc()), 12);
    cp.expect("call");
    cp.verify();

    cell0.set(3, ac);
    cell1.set(30, ac);
    assert_eq!(s.get(ac.oc()), 33);
    cp.expect("call");
    cp.verify();
    assert_eq!(s.get(ac.oc()), 33);
    cp.expect("call");
    cp.verify();
}

#[test]
fn from_future() {
    let mut dc = Runtime::new();
    let (sender, receiver) = channel();
    let s = Obs::from_future(receiver);
    assert_eq!(s.get(dc.ac().oc()), Poll::Pending);
    sender.send(10).unwrap();
    assert_eq!(s.get(dc.ac().oc()), Poll::Ready(Ok(10)));
}

#[test]
fn from_future_send_in_get() {
    let mut dc = Runtime::new();
    let (sender, receiver) = channel();
    let s = Obs::from_future(receiver);
    let mut ac = dc.ac();
    let oc = ac.oc();
    assert_eq!(s.get(oc), Poll::Pending);
    sender.send(10).unwrap();
    assert_eq!(s.get(oc), Poll::Pending);
    assert_eq!(s.get(dc.ac().oc()), Poll::Ready(Ok(10)));
}

#[test]
fn subscribe() {
    let mut dc = Runtime::new();
    let cell = ObsCell::new(0);

    let rs = Rc::new(RefCell::new(Vec::new()));
    let _ss = cell.obs().subscribe({
        let rs = rs.clone();
        move |&x| rs.borrow_mut().push(x)
    });
    dc.update();

    cell.set(1, &mut dc.ac());
    dc.update();

    cell.set(2, &mut dc.ac());
    dc.update();

    cell.set(3, &mut dc.ac());
    dc.update();

    assert_eq!(&*rs.borrow(), &vec![0, 1, 2, 3]);
}

#[test]
fn subscribe_2() {
    for _ in 0..2 {
        let mut dc = Runtime::new();
        let cell = ObsCell::new(0);

        let rs = Rc::new(RefCell::new(Vec::new()));
        let _ss = cell.obs().subscribe({
            let rs = rs.clone();
            move |&x| rs.borrow_mut().push(x)
        });
        dc.update();

        cell.set(1, &mut dc.ac());
        dc.update();

        assert_eq!(&*rs.borrow(), &vec![0, 1]);
    }
}

#[test]
fn collect_vec() {
    let mut dc = Runtime::new();
    let cell = ObsCell::new(0);

    let ss = cell.obs().collect_vec();
    dc.update();

    cell.set(1, &mut dc.ac());
    dc.update();

    cell.set(2, &mut dc.ac());
    dc.update();

    cell.set(3, &mut dc.ac());
    dc.update();

    assert_eq!(ss.stop(dc.ac().oc()), vec![0, 1, 2, 3]);
}

#[test]
fn cached_collect() {
    let mut dc = Runtime::new();
    let cell = ObsCell::new(0);

    let ss = cell.obs().map_value(|x| x + 1).cached().collect_vec();
    dc.update();

    cell.set(1, &mut dc.ac());
    dc.update();

    cell.set(2, &mut dc.ac());
    dc.update();

    cell.set(3, &mut dc.ac());
    dc.update();

    assert_eq!(ss.stop(dc.ac().oc()), vec![1, 2, 3, 4]);
}

#[test]
fn deep() {
    let mut dc = Runtime::new();
    const DEPTH: usize = 100;
    const COUNT: usize = 100;
    let cell = ObsCell::new(0);
    let mut s = cell.obs();
    for _ in 0..DEPTH {
        s = s.map_value(|x| x + 1).cached();
    }
    let rs = Rc::new(RefCell::new(Vec::new()));
    let _s = s.subscribe({
        let rs = rs.clone();
        move |&x| rs.borrow_mut().push(x)
    });
    for i in 0..COUNT {
        cell.set(i, &mut dc.ac());
        dc.update();
    }
    let e: Vec<_> = (0..COUNT).map(|x| x + DEPTH).collect();
    assert_eq!(&*rs.borrow(), &e);
}

// #[test]
// fn deep_2() {
//     for _ in 0..2 {
//         dc_test(|dc| {
//             const DEPTH: usize = 100;
//             const COUNT: usize = 1000;

//             let cell = ObsCell::new(0);
//             let mut s = cell.obs();
//             for _ in 0..DEPTH {
//                 s = s.map(|x| x + 1).cached();
//             }
//             let count = Rc::new(Cell::new(0));

//             let _s = s.subscribe({
//                 let count = count.clone();
//                 move |_| {
//                     count.set(count.get() + 1);
//                 }
//             });
//             for i in 0..COUNT {
//                 cell.set(i, &mut dc.ac());
//                 dc.update();
//             }
//             drop(_s);
//             assert_eq!(count.get(), COUNT);
//         });
//     }
// }

// #[test]
// fn leak_check() {
//     dc_test(|dc| {
//         let cell = ObsCell::new(0);
//         for i in 0..10 {
//             {
//                 let mut ss = Vec::new();
//                 for _ in 0..10 {
//                     ss.push(cell.obs().map(|x| x + 1).subscribe(|_| {}));
//                 }
//                 for i in 0..10 {
//                     cell.set(i, &mut dc.ac());
//                     dc.update();
//                 }
//             }
//             dc.dump();
//             if i == 4 {
//                 panic!("check point");
//             }
//         }
//     });
// }
