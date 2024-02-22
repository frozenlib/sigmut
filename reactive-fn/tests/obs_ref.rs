use std::{
    cell::RefCell,
    fmt::{Debug, Display},
};

use assert_call::{call, Call, CallRecorder};
use derive_ex::derive_ex;
use parse_display::Display;
use reactive_fn::{
    core::{ObsRef, ObsRefBuilder, Runtime},
    ObsContext,
};

#[derive(Display, Clone, Copy, Eq, PartialEq)]
#[display("large")]
struct Large([usize; 20]);

impl Large {
    fn new() -> Self {
        let mut val = [0; 20];
        (0..20).for_each(|i| val[i] = i);
        Large(val)
    }
}
impl Debug for Large {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Large").finish()
    }
}

#[derive(Eq, PartialEq, Debug, Display)]
#[derive_ex(Deref)]
struct Value<T: Display>(T);

impl<T: Display> Drop for Value<T> {
    fn drop(&mut self) {
        call!("drop {}", self.0);
    }
}

fn e_drop(value: impl Display) -> Call {
    Call::id(format!("drop {value}"))
}

#[test]
fn from_value_small() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let value = Value(10);

    let or = ObsRef::from_value(value, &rt.oc());
    assert_eq!(**or, 10);
    c.verify(());
    drop(or);
    c.verify(e_drop(10));
}

#[test]
fn from_value_large() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let large = Large::new();
    let value = Value(large);
    let oc = rt.oc();
    let or = ObsRef::from_value(value, &oc);
    assert_eq!(**or, large);
    c.verify(());
    drop(or);
    c.verify(e_drop(large));
}

#[test]
fn from_value_non_static_small() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let value = Value(10);

    let or = ObsRef::from_value_non_static(&value, &rt.oc());
    assert_eq!(***or, 10);
    c.verify(());
    drop(or);
    c.verify(());
    drop(value);
    c.verify(e_drop(10));
}

#[test]
fn from_value_non_static_large() {
    #[derive(Eq, PartialEq, Debug, Display)]
    #[display("large2")]
    struct Large2<'a> {
        a: Value<Large>,
        b: &'a u32,
    }

    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let value2 = Value(Large2 {
        a: Value(Large::new()),
        b: &10,
    });

    let or = ObsRef::from_value_non_static(value2, &rt.oc());
    let _: &Large2 = &or;
    c.verify(());
    drop(or);
    c.verify([e_drop("large2"), e_drop("large")]);
}

#[test]
fn from_ref() {
    let x = 10;
    let or: ObsRef<_> = (&x).into();
    assert_eq!(*or, 10);
}

#[test]
fn from_ref_cell() {
    let x = RefCell::new(10);
    let or: ObsRef<_> = x.borrow().into();
    assert_eq!(*or, 10);
}

#[test]
fn from_value_small_map() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let value = Value(10);
    let or = ObsRefBuilder::from_value(value, &mut rt.oc())
        .map(|x| x)
        .build();

    c.verify(());
    assert_eq!(**or, 10);
    drop(or);
    c.verify(e_drop(10));
}

#[test]
fn from_value_large_map() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let large = Large::new();
    let or = ObsRefBuilder::from_value(Value(large), &mut rt.oc())
        .map(|x| x)
        .build();
    c.verify(());
    assert_eq!(**or, large);
    drop(or);
    c.verify(e_drop(large));
}

#[test]
fn from_value_non_static_small_map() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let value = Value(10);
    let or = ObsRefBuilder::from_value_non_static(&value, &mut rt.oc())
        .map(|x| x)
        .build();
    c.verify(());
    assert_eq!(***or, 10);
    drop(or);
    c.verify(());
    drop(value);
    c.verify(e_drop(10));
}

#[test]
fn from_value_non_static_large_map() {
    #[derive(Eq, PartialEq, Debug, Display)]
    #[display("large2")]
    struct Large2<'a> {
        a: Value<Large>,
        b: &'a u32,
    }

    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let value2 = Value(Large2 {
        a: Value(Large::new()),
        b: &10,
    });

    let or = ObsRefBuilder::from_value_non_static(value2, &mut rt.oc())
        .map(|x| x)
        .build();
    let _: &Large2 = &or;
    c.verify(());
    drop(or);
    c.verify([e_drop("large2"), e_drop("large")]);
}

#[test]
fn from_ref_map() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let x = 10;
    let or = ObsRefBuilder::from_ref(&x, &mut rt.oc()).map(|x| x).build();
    assert_eq!(*or, 10);
    c.verify(());
}

#[test]
fn from_ref_cell_map() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let x = RefCell::new(10);
    let or = ObsRefBuilder::from_ref_cell(x.borrow(), &mut rt.oc())
        .map(|x| x)
        .build();
    assert_eq!(*or, 10);
    c.verify(());
}

#[test]
fn from_value_small_map_ref_value_small() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let value = Value(10);
    let or = ObsRefBuilder::from_value(value, &mut rt.oc())
        .map_ref(|_, oc, _| ObsRef::from_value(Value(20), oc))
        .build();
    c.verify(e_drop(10));
    assert_eq!(**or, 20);
    drop(or);
    c.verify(e_drop(20));
}

#[test]
fn from_value_small_map_ref_value_large() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let value = Value(10);
    let or = ObsRefBuilder::from_value(value, &mut rt.oc())
        .map_ref(|_, oc, _| ObsRef::from_value(Value(Large::new()), oc))
        .build();
    c.verify(e_drop(10));
    let _: &Large = &or;
    drop(or);
    c.verify(e_drop("large"));
}

// TODO

// #[test]
// fn from_value_map_ref_value() {
//     let mut c = CallRecorder::new();
//     let mut rt = Runtime::new();

//     let value = Value(10);
//     let or = ObsRef::from_value_map_ref(
//         value,
//         |_, oc| ObsRef::from_value(Value(5), oc),
//         &mut rt.oc(),
//     );
//     c.verify(e_drop(10));
//     assert_eq!(**or, 5);
//     drop(or);
//     c.verify(e_drop(5));
// }

// #[test]
// fn from_value_map_ref_value_large() {
//     let mut c = CallRecorder::new();
//     let mut rt = Runtime::new();

//     let value = Value(10);
//     let or = ObsRef::from_value_map_ref(
//         value,
//         |_, oc| ObsRef::from_value(Value(Large::new()), oc),
//         &mut rt.oc(),
//     );
//     dbg!(&or);

//     c.verify(e_drop(10));
//     let _: &Large = &or;
//     drop(or);
//     c.verify(e_drop("large"));
// }

// #[test]
// fn from_value_map_ref_ref() {
//     let mut c = CallRecorder::new();
//     let mut rt = Runtime::new();

//     let value = Value(10);
//     let or = ObsRef::from_value_map_ref(value, |x, _| x.into(), &mut rt.oc());
//     assert_eq!(**or, 10);
//     c.verify(());
//     drop(or);
//     c.verify(e_drop(10));
// }

// #[test]
// fn from_value_map_ref_ref_cell() {
//     let mut c = CallRecorder::new();
//     let mut rt = Runtime::new();

//     #[derive(Display)]
//     #[display("x")]
//     struct X(RefCell<usize>);

//     let value = Value(X(RefCell::new(10)));
//     let or = ObsRef::from_value_map_ref(value, |x, _| x.0 .0.borrow().into(), &mut rt.oc());
//     assert_eq!(*or, 10);
//     c.verify(());
//     drop(or);
//     c.verify(e_drop("x"));
// }
// #[test]
// fn from_value_map_ref_nested() {
//     let mut c = CallRecorder::new();
//     let mut rt = Runtime::new();

//     let value = Value(10);
//     let or = ObsRef::from_value_map_ref(
//         value,
//         |x, oc| {
//             ObsRef::from_value_non_static_map_ref(x, |_, oc| ObsRef::from_value(Value(20), oc), oc)
//         },
//         &mut rt.oc(),
//     );
//     c.verify(e_drop(10));
//     assert_eq!(**or, 20);
//     drop(or);
//     c.verify(e_drop(20));
// }

#[test]
fn bad() {
    struct X;

    let mut rt = Runtime::new();
    let oc = &mut rt.oc();
    let value = X;
    let or = ObsRefBuilder::from_value_non_static(&value, oc)
        .map_ref(|_, oc, _| ObsRef::from_value_non_static(10, oc))
        .build();
    drop(value);
    drop(or);

    // drop(oc);

    // assert_eq!(**or, 10);
    // c.verify(());
    // drop(or);
    // c.verify(e_drop(10));
}
