use std::{
    cell::RefCell,
    fmt::{Debug, Display},
    hint::black_box,
};

use assert_call::{call, Call, CallRecorder};
use derive_ex::derive_ex;
use parse_display::Display;
use reactive_fn::{
    core::{ObsRef, ObsRefBuilder, Runtime},
    ObsContext,
};
use rstest::rstest;

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
    let oc = &rt.oc();
    check(1u8, oc);
    check(1u16, oc);
    check(1u32, oc);
    check(1u64, oc);
    check(1u128, oc);

    fn check<T: PartialEq + Debug + Copy + 'static>(value: T, oc: &ObsContext) {
        assert_eq!(*black_box(ObsRef::from_value(value, oc)), value);
    }
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

    let value = Value(LARGE);
    let oc = rt.oc();
    let or = ObsRef::from_value(value, &oc);
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
        a: Value(LARGE),
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
fn map() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let value = Value(10);
    let oc = &mut rt.oc();
    let or = ObsRef::map((&value).into(), |x| x, oc);
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
    let oc = &mut rt.oc();
    let or = ObsRef::map((&value).into(), |x| &x.b, oc);
    assert_eq!(*or, 20);
}

#[test]
fn map_with_allocate() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();

    let value = Value(10);
    let oc = &mut rt.oc();
    let or = ObsRef::map(ObsRef::from_value(value, oc), |x| x, oc);
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
    let oc = &mut rt.oc();
    let or = ObsRef::map_ref(
        ObsRef::from_value(value, oc),
        |_, oc, _| ObsRef::from_value_non_static(Value(20), oc),
        oc,
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
    let oc = &mut rt.oc();
    let or = ObsRef::map_ref(
        ObsRef::from_value(value, oc),
        |_, oc, _| ObsRef::from_value(Value(20), oc),
        oc,
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

    let x = ObsRefBuilder::from_value(Value(10), &mut rt.oc())
        .map_ref(|_, oc, _| ObsRef::from_value_non_static(Value(20), oc))
        .map_ref(|_, oc, _| ObsRef::from_value_non_static(Value(30), oc))
        .build();
    c.verify(());
    assert_eq!(**x, 30);
    drop(x);
    c.verify([e_drop(30), e_drop(20), e_drop(10)]);
}

trait ObsRefFn {
    fn call<'a, T: Debug + ?Sized>(&self, value: ObsRef<'a, T>, oc: &mut ObsContext<'a>);
}

#[derive(Clone, Copy, Debug)]
enum ObsRefKind {
    ValueSmall,
    ValueLarge,
    ValueNonStaticSmall,
    ValueNonStaticLarge,
    Ref,
    RefCell,
}
impl ObsRefKind {
    fn init(self, f: impl ObsRefFn) {
        let mut rt = Runtime::new();
        let oc = &mut rt.oc();
        match self {
            ObsRefKind::ValueSmall => f.call(ObsRef::from_value(10, oc), oc),
            ObsRefKind::ValueLarge => f.call(ObsRef::from_value(LARGE, oc), oc),
            ObsRefKind::ValueNonStaticSmall => f.call(ObsRef::from_value_non_static(&10, oc), oc),
            ObsRefKind::ValueNonStaticLarge => {
                f.call(ObsRef::from_value_non_static(&LARGE, oc), oc)
            }
            ObsRefKind::Ref => f.call((&10).into(), oc),
            ObsRefKind::RefCell => f.call(RefCell::new(10).borrow().into(), oc),
        }
    }
}

impl ObsRefFn for () {
    fn call<'a, T: Debug + ?Sized>(&self, value: ObsRef<'a, T>, _oc: &mut ObsContext<'a>) {
        black_box(&*value);
    }
}

#[rstest]
fn each_from_value(
    #[values(
        ObsRefKind::ValueSmall,
        ObsRefKind::ValueLarge,
        ObsRefKind::ValueNonStaticSmall,
        ObsRefKind::ValueNonStaticLarge,
        ObsRefKind::Ref,
        ObsRefKind::RefCell
    )]
    kind: ObsRefKind,
) {
    kind.init(());
}

struct ObsRefFnMap;
impl ObsRefFn for ObsRefFnMap {
    fn call<'a, T: Debug + ?Sized>(&self, value: ObsRef<'a, T>, oc: &mut ObsContext<'a>) {
        bbox(ObsRef::map(value, |x| x, oc));
    }
}

#[rstest]
fn each_map(
    #[values(
        ObsRefKind::ValueSmall,
        ObsRefKind::ValueLarge,
        ObsRefKind::ValueNonStaticSmall,
        ObsRefKind::ValueNonStaticLarge,
        ObsRefKind::Ref,
        ObsRefKind::RefCell
    )]
    kind: ObsRefKind,
) {
    kind.init(ObsRefFnMap);
}

struct ObsRefFnMapRef(ObsRefKind);
impl ObsRefFn for ObsRefFnMapRef {
    fn call<'a, T: Debug + ?Sized>(&self, value: ObsRef<'a, T>, oc: &mut ObsContext<'a>) {
        match self.0 {
            ObsRefKind::ValueSmall => {
                bbox(ObsRef::map_ref(
                    value,
                    |_x, oc, _| ObsRef::from_value(10, oc),
                    oc,
                ));
            }
            ObsRefKind::ValueLarge => {
                bbox(ObsRef::map_ref(
                    value,
                    |_x, oc, _| ObsRef::from_value(LARGE, oc),
                    oc,
                ));
            }
            ObsRefKind::ValueNonStaticSmall => {
                bbox(ObsRef::map_ref(
                    value,
                    |_x, oc, _| ObsRef::from_value_non_static(&10, oc),
                    oc,
                ));
            }
            ObsRefKind::ValueNonStaticLarge => {
                bbox(ObsRef::map_ref(
                    value,
                    |_x, oc, _| ObsRef::from_value_non_static(&LARGE, oc),
                    oc,
                ));
            }
            ObsRefKind::Ref => {
                bbox(ObsRef::map_ref(value, |x, _oc, _| x.into(), oc));
            }
            ObsRefKind::RefCell => {
                bbox(
                    ObsRefBuilder::new(value, oc)
                        .map_ref(|_x, oc, _| ObsRef::from_value(RefCell::new(10), oc))
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
        ObsRefKind::ValueSmall,
        ObsRefKind::ValueLarge,
        ObsRefKind::ValueNonStaticSmall,
        ObsRefKind::ValueNonStaticLarge,
        ObsRefKind::Ref,
        ObsRefKind::RefCell
    )]
    init: ObsRefKind,
    #[values(
        ObsRefKind::ValueSmall,
        ObsRefKind::ValueLarge,
        ObsRefKind::ValueNonStaticSmall,
        ObsRefKind::ValueNonStaticLarge,
        ObsRefKind::Ref,
        ObsRefKind::RefCell
    )]
    map_ref: ObsRefKind,
) {
    init.init(ObsRefFnMapRef(map_ref));
}

fn _lifetime_covariant() {
    fn _f<'a: 'b, 'b, T>(x: ObsRef<'a, T>) -> ObsRef<'b, T> {
        x
    }
}

fn bbox<T: ?Sized + Debug>(value: ObsRef<T>) {
    black_box(format!("{:?}", value));
    black_box(&*value);
}
