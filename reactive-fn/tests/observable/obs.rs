use assert_call::{call, CallRecorder};
use futures::channel::oneshot::channel;
use reactive_fn::core::Runtime;
use reactive_fn::{wait_for_update, Obs, ObsCell};
use rt_local::runtime::core::test;
use std::task::Poll;
use std::{cell::RefCell, rc::Rc};

#[test]
fn new() {
    let mut cp = CallRecorder::new();
    let mut rt = Runtime::new();
    let cell0 = ObsCell::new(1);
    let cell1 = ObsCell::new(10);
    let s = Obs::new({
        let cell0 = cell0.obs();
        let cell1 = cell1.obs();
        move |oc| {
            call!("call");
            cell0.get(oc) + cell1.get(oc)
        }
    });
    let ac = &mut rt.ac();

    assert_eq!(s.get(&mut ac.oc()), 11);
    cp.verify("call");
    assert_eq!(s.get(&mut ac.oc()), 11);
    cp.verify(());
    cell0.set(2, ac);
    assert_eq!(s.get(&mut ac.oc()), 12);
    cp.verify("call");
    assert_eq!(s.get(&mut ac.oc()), 12);
    cp.verify(());
    cell0.set(3, ac);
    cell1.set(30, ac);
    assert_eq!(s.get(&mut ac.oc()), 33);
    cp.verify("call");
    assert_eq!(s.get(&mut ac.oc()), 33);
    cp.verify(());
}

#[test]
fn from_get() {
    let mut cp = CallRecorder::new();
    let mut rt = Runtime::new();
    let cell0 = ObsCell::new(1);
    let cell1 = ObsCell::new(10);
    let s = Obs::from_get({
        let cell0 = cell0.obs();
        let cell1 = cell1.obs();
        move |oc| {
            call!("call");
            cell0.get(oc) + cell1.get(oc)
        }
    });
    let ac = &mut rt.ac();

    assert_eq!(s.get(&mut ac.oc()), 11);
    cp.verify("call");
    assert_eq!(s.get(&mut ac.oc()), 11);
    cp.verify("call");
    cell0.set(2, ac);
    assert_eq!(s.get(&mut ac.oc()), 12);
    cp.verify("call");
    assert_eq!(s.get(&mut ac.oc()), 12);
    cp.verify("call");
    cell0.set(3, ac);
    cell1.set(30, ac);
    assert_eq!(s.get(&mut ac.oc()), 33);
    cp.verify("call");
    assert_eq!(s.get(&mut ac.oc()), 33);
    cp.verify("call");
}

#[test]
async fn from_future() {
    let mut rt = Runtime::new();
    let (sender, receiver) = channel();
    let s = Obs::from_future(receiver);
    assert_eq!(s.get(&mut rt.oc()), Poll::Pending);
    sender.send(10).unwrap();
    rt.run(|_rt| async {
        wait_for_update().await;
    })
    .await;
    assert_eq!(s.get(&mut rt.oc()), Poll::Ready(Ok(10)));
}

#[test]
fn subscribe() {
    let mut rt = Runtime::new();
    let cell = ObsCell::new(0);

    let rs = Rc::new(RefCell::new(Vec::new()));
    let _ss = cell.obs().subscribe({
        let rs = rs.clone();
        move |&x| rs.borrow_mut().push(x)
    });
    rt.update();

    cell.set(1, &mut rt.ac());
    rt.update();

    cell.set(2, &mut rt.ac());
    rt.update();

    cell.set(3, &mut rt.ac());
    rt.update();

    assert_eq!(&*rs.borrow(), &vec![0, 1, 2, 3]);
}

#[test]
fn subscribe_2() {
    for _ in 0..2 {
        let mut rt = Runtime::new();
        let cell = ObsCell::new(0);

        let rs = Rc::new(RefCell::new(Vec::new()));
        let _ss = cell.obs().subscribe({
            let rs = rs.clone();
            move |&x| rs.borrow_mut().push(x)
        });
        rt.update();

        cell.set(1, &mut rt.ac());
        rt.update();

        assert_eq!(&*rs.borrow(), &vec![0, 1]);
    }
}

#[test]
fn collect_vec() {
    let mut rt = Runtime::new();
    let cell = ObsCell::new(0);

    let ss = cell.obs().collect_vec();
    rt.update();

    cell.set(1, &mut rt.ac());
    rt.update();

    cell.set(2, &mut rt.ac());
    rt.update();

    cell.set(3, &mut rt.ac());
    rt.update();

    assert_eq!(ss.stop(&mut rt.uc()), vec![0, 1, 2, 3]);
}

#[test]
fn memo_collect() {
    let mut rt = Runtime::new();
    let cell = ObsCell::new(0);

    let ss = cell.obs().map_value(|x| x + 1).memo().collect_vec();
    rt.update();

    cell.set(1, &mut rt.ac());
    rt.update();

    cell.set(2, &mut rt.ac());
    rt.update();

    cell.set(3, &mut rt.ac());
    rt.update();

    assert_eq!(ss.stop(&mut rt.uc()), vec![1, 2, 3, 4]);
}

#[test]
fn deep() {
    let mut rt = Runtime::new();
    const DEPTH: usize = 100;
    const COUNT: usize = 100;
    let cell = ObsCell::new(0);
    let mut s = cell.obs();
    for _ in 0..DEPTH {
        s = s.map_value(|x| x + 1).memo();
    }
    let rs = Rc::new(RefCell::new(Vec::new()));
    let _s = s.subscribe({
        let rs = rs.clone();
        move |&x| rs.borrow_mut().push(x)
    });
    for i in 0..COUNT {
        cell.set(i, &mut rt.ac());
        rt.update();
    }
    let e: Vec<_> = (0..COUNT).map(|x| x + DEPTH).collect();
    assert_eq!(&*rs.borrow(), &e);
}

// #[test]
// fn deep_2() {
//     for _ in 0..2 {
//         dc_test(|rt| {
//             const DEPTH: usize = 100;
//             const COUNT: usize = 1000;

//             let cell = ObsCell::new(0);
//             let mut s = cell.obs();
//             for _ in 0..DEPTH {
//                 s = s.map(|x| x + 1).memo();
//             }
//             let count = Rc::new(Cell::new(0));

//             let _s = s.subscribe({
//                 let count = count.clone();
//                 move |_| {
//                     count.set(count.get() + 1);
//                 }
//             });
//             for i in 0..COUNT {
//                 cell.set(i, &mut rt.ac());
//                 rt.update();
//             }
//             drop(_s);
//             assert_eq!(count.get(), COUNT);
//         });
//     }
// }

// #[test]
// fn leak_check() {
//     dc_test(|rt| {
//         let cell = ObsCell::new(0);
//         for i in 0..10 {
//             {
//                 let mut ss = Vec::new();
//                 for _ in 0..10 {
//                     ss.push(cell.obs().map(|x| x + 1).subscribe(|_| {}));
//                 }
//                 for i in 0..10 {
//                     cell.set(i, &mut rt.ac());
//                     rt.update();
//                 }
//             }
//             rt.dump();
//             if i == 4 {
//                 panic!("check point");
//             }
//         }
//     });
// }
