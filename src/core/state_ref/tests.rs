use std::{
    cell::RefCell,
    fmt::{Debug, Display},
    hint::black_box,
};

use assert_call::{call, Call, CallRecorder};
use derive_ex::derive_ex;
use parse_display::Display;
use rstest::rstest;

use crate::{
    core::{Runtime, StateRefBuilder},
    SignalContext, StateRef,
};

#[derive(Display, Clone, Copy, Eq, PartialEq)]
#[display("large")]
struct Large([usize; 20]);

const LARGE: Large = Large([
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19,
]);

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
fn move_value() {
    let mut rt = Runtime::new();
    let sc = &rt.sc();
    check(1u8, sc);
    check(1u16, sc);
    check(1u32, sc);
    check(1u64, sc);
    check(1u128, sc);

    fn check<T: PartialEq + Debug + Copy + 'static>(value: T, sc: &SignalContext) {
        assert_eq!(*black_box(StateRef::from_value(value, sc)), value);
    }
}

#[test]
fn into_owned() {
    let mut rt = Runtime::new();
    let sc = &rt.sc();

    #[derive(Debug)]
    struct X(u8);

    impl Clone for X {
        fn clone(&self) -> Self {
            unreachable!()
        }
    }
    let x = StateRef::from_value(X(10), sc).into_owned();
    assert_eq!(x.0, 10);
}

#[test]
fn from_value_small() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let value = Value(10);

    let or = StateRef::from_value(value, &rt.sc());
    assert_eq!(**or, 10);
    c.verify(());
    drop(or);
    c.verify(e_drop(10));
}

#[test]
fn from_value_large() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let value = Value(LARGE);
    let sc = rt.sc();
    let or = StateRef::from_value(value, &sc);
    assert_eq!(**or, LARGE);
    c.verify(());
    drop(or);
    c.verify(e_drop(LARGE));
}

#[test]
fn from_value_non_static_small() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let value = Value(10);

    let or = StateRef::from_value_non_static(&value, &rt.sc());
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
        a: Value(LARGE),
        b: &10,
    });

    let or = StateRef::from_value_non_static(value2, &rt.sc());
    let _: &Large2 = &or;
    c.verify(());
    drop(or);
    c.verify([e_drop("large2"), e_drop("large")]);
}

#[test]
fn from_ref() {
    let x = 10;
    let or: StateRef<_> = (&x).into();
    assert_eq!(*or, 10);
}

#[test]
fn from_ref_cell() {
    let x = RefCell::new(10);
    let or: StateRef<_> = x.borrow().into();
    assert_eq!(*or, 10);
}

#[test]
fn map() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let value = Value(10);
    let sc = &mut rt.sc();
    let or = StateRef::map((&value).into(), |x| x, sc);
    c.verify(());
    assert_eq!(**or, 10);
    drop(or);
    c.verify(());
    drop(value);
    c.verify(e_drop(10));
}

#[test]
fn map_to_member() {
    let mut _c = CallRecorder::new();
    let mut rt = Runtime::new();

    struct X {
        _a: u32,
        b: u32,
    }

    let value = X { _a: 10, b: 20 };
    let sc = &mut rt.sc();
    let or = StateRef::map((&value).into(), |x| &x.b, sc);
    assert_eq!(*or, 20);
}

#[test]
fn map_with_allocate() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let value = Value(10);
    let sc = &mut rt.sc();
    let or = StateRef::map(StateRef::from_value(value, sc), |x| x, sc);
    c.verify(());
    assert_eq!(**or, 10);
    drop(or);
    c.verify(e_drop(10));
}

#[test]
fn map_ref_non_static() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let value = Value(10);
    let sc = &mut rt.sc();
    let or = StateRef::map_ref(
        StateRef::from_value(value, sc),
        |_, sc, _| StateRef::from_value_non_static(Value(20), sc),
        sc,
    );
    c.verify(());
    assert_eq!(**or, 20);
    drop(or);
    c.verify([e_drop(20), e_drop(10)]);
}

#[test]
fn map_ref_static() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let value = Value(10);
    let sc = &mut rt.sc();
    let or = StateRef::map_ref(
        StateRef::from_value(value, sc),
        |_, sc, _| StateRef::from_value(Value(20), sc),
        sc,
    );
    c.verify(e_drop(10));
    assert_eq!(**or, 20);
    drop(or);
    c.verify(e_drop(20));
}

#[test]
fn nested() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let x = StateRefBuilder::from_value(Value(10), &mut rt.sc())
        .map_ref(|_, sc, _| StateRef::from_value_non_static(Value(20), sc))
        .map_ref(|_, sc, _| StateRef::from_value_non_static(Value(30), sc))
        .build();
    c.verify(());
    assert_eq!(**x, 30);
    drop(x);
    c.verify([e_drop(30), e_drop(20), e_drop(10)]);
}

#[test]
fn owned_borrow() {
    let mut rt = Runtime::new();

    struct X(i32);
    impl X {
        fn borrow<'a, 's: 'a>(&'a self, _sc: &mut SignalContext<'s>) -> StateRef<'a, i32> {
            (&self.0).into()
        }
    }

    let x = X(10);
    let _r = StateRefBuilder::from_value(x, &mut rt.sc())
        .map_ref(|x, sc, _| x.borrow(sc))
        .build();
}

#[test]
fn owned_borrow_2() {
    let mut rt = Runtime::new();

    struct X(Y);
    impl X {
        fn borrow<'a, 's: 'a>(&'a self, _sc: &mut SignalContext<'s>) -> StateRef<'a, Y> {
            (&self.0).into()
        }
    }

    struct Y(i32);
    impl Y {
        fn borrow<'a, 's: 'a>(&'a self, _sc: &mut SignalContext<'s>) -> StateRef<'a, i32> {
            (&self.0).into()
        }
    }

    let x = X(Y(10));
    let _r = StateRefBuilder::from_value(x, &mut rt.sc())
        .map_ref(|x, sc, _| x.borrow(sc))
        .map_ref(|y, sc, _| y.borrow(sc))
        .build();
}

trait StateRefFn {
    fn call<'a, T: Debug + ?Sized>(&self, value: StateRef<'a, T>, sc: &mut SignalContext<'a>);
}

#[derive(Clone, Copy, Debug)]
enum StateRefKind {
    ValueSmall,
    ValueLarge,
    ValueNonStaticSmall,
    ValueNonStaticLarge,
    Ref,
    RefCell,
}
impl StateRefKind {
    fn init(self, f: impl StateRefFn) {
        let mut rt = Runtime::new();
        let sc = &mut rt.sc();
        match self {
            StateRefKind::ValueSmall => f.call(StateRef::from_value(10, sc), sc),
            StateRefKind::ValueLarge => f.call(StateRef::from_value(LARGE, sc), sc),
            StateRefKind::ValueNonStaticSmall => {
                f.call(StateRef::from_value_non_static(&10, sc), sc)
            }
            StateRefKind::ValueNonStaticLarge => {
                f.call(StateRef::from_value_non_static(&LARGE, sc), sc)
            }
            StateRefKind::Ref => f.call((&10).into(), sc),
            StateRefKind::RefCell => f.call(RefCell::new(10).borrow().into(), sc),
        }
    }
}

impl StateRefFn for () {
    fn call<'a, T: Debug + ?Sized>(&self, value: StateRef<'a, T>, _oc: &mut SignalContext<'a>) {
        black_box(&*value);
    }
}

#[rstest]
fn each_from_value(
    #[values(
        StateRefKind::ValueSmall,
        StateRefKind::ValueLarge,
        StateRefKind::ValueNonStaticSmall,
        StateRefKind::ValueNonStaticLarge,
        StateRefKind::Ref,
        StateRefKind::RefCell
    )]
    kind: StateRefKind,
) {
    kind.init(());
}

struct StateRefFnMap;
impl StateRefFn for StateRefFnMap {
    fn call<'a, T: Debug + ?Sized>(&self, value: StateRef<'a, T>, sc: &mut SignalContext<'a>) {
        bbox(StateRef::map(value, |x| x, sc));
    }
}

#[rstest]
fn each_map(
    #[values(
        StateRefKind::ValueSmall,
        StateRefKind::ValueLarge,
        StateRefKind::ValueNonStaticSmall,
        StateRefKind::ValueNonStaticLarge,
        StateRefKind::Ref,
        StateRefKind::RefCell
    )]
    kind: StateRefKind,
) {
    kind.init(StateRefFnMap);
}

struct StateRefFnMapRef(StateRefKind);
impl StateRefFn for StateRefFnMapRef {
    fn call<'a, T: Debug + ?Sized>(&self, value: StateRef<'a, T>, sc: &mut SignalContext<'a>) {
        match self.0 {
            StateRefKind::ValueSmall => {
                bbox(StateRef::map_ref(
                    value,
                    |_x, sc, _| StateRef::from_value(10, sc),
                    sc,
                ));
            }
            StateRefKind::ValueLarge => {
                bbox(StateRef::map_ref(
                    value,
                    |_x, sc, _| StateRef::from_value(LARGE, sc),
                    sc,
                ));
            }
            StateRefKind::ValueNonStaticSmall => {
                bbox(StateRef::map_ref(
                    value,
                    |_x, sc, _| StateRef::from_value_non_static(&10, sc),
                    sc,
                ));
            }
            StateRefKind::ValueNonStaticLarge => {
                bbox(StateRef::map_ref(
                    value,
                    |_x, sc, _| StateRef::from_value_non_static(&LARGE, sc),
                    sc,
                ));
            }
            StateRefKind::Ref => {
                bbox(StateRef::map_ref(value, |x, _oc, _| x.into(), sc));
            }
            StateRefKind::RefCell => {
                bbox(
                    StateRefBuilder::new(value, sc)
                        .map_ref(|_x, sc, _| StateRef::from_value(RefCell::new(10), sc))
                        .map_ref(|x, _oc, _| x.borrow().into())
                        .build(),
                );
            }
        }
    }
}

#[rstest]
fn each_map_ref(
    #[values(
        StateRefKind::ValueSmall,
        StateRefKind::ValueLarge,
        StateRefKind::ValueNonStaticSmall,
        StateRefKind::ValueNonStaticLarge,
        StateRefKind::Ref,
        StateRefKind::RefCell
    )]
    init: StateRefKind,
    #[values(
        StateRefKind::ValueSmall,
        StateRefKind::ValueLarge,
        StateRefKind::ValueNonStaticSmall,
        StateRefKind::ValueNonStaticLarge,
        StateRefKind::Ref,
        StateRefKind::RefCell
    )]
    map_ref: StateRefKind,
) {
    init.init(StateRefFnMapRef(map_ref));
}

fn _lifetime_covariant() {
    fn _f<'a: 'b, 'b, T>(x: StateRef<'a, T>) -> StateRef<'b, T> {
        x
    }
}

fn bbox<T: ?Sized + Debug>(value: StateRef<T>) {
    black_box(format!("{value:?}"));
    black_box(&*value);
}
