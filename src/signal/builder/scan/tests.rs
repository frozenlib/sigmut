use super::*;
use crate::{
    SignalBuilder, State,
    core::{Runtime, SourceBindings},
};
use pretty_assertions::assert_eq;
use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
};

#[test]
fn scan_is_lazy_and_retains_mutable_local_captures() {
    let mut rt = Runtime::new();
    let source = State::new(10);
    let calls = Rc::new(Cell::new(0));
    let weak_calls = Rc::downgrade(&calls);
    let signal = SignalBuilder::from_scan(0, {
        let source = source.clone();
        let calls = calls.clone();
        let mut count = 0;
        move |value, sc| {
            count += 1;
            calls.set(count);
            *value = source.get(sc) + count;
        }
    })
    .build();

    rt.flush();
    assert_eq!(calls.get(), 0);
    assert_eq!(
        [signal.get(&mut rt.sc()), signal.get(&mut rt.sc())],
        [11, 11]
    );
    assert_eq!(calls.get(), 1);

    source.set(20, rt.ac());
    assert_eq!(signal.get(&mut rt.sc()), 22);
    source.set(30, rt.ac());
    rt.flush();
    assert_eq!(calls.get(), 2);
    assert_eq!(signal.get(&mut rt.sc()), 33);

    drop(calls);
    let clone = signal.clone();
    drop(signal);
    assert!(weak_calls.upgrade().is_some());
    drop(clone);
    assert!(weak_calls.upgrade().is_none());
}

#[test]
fn scan_panic_preserves_state_and_callback_until_discard() {
    let mut rt = Runtime::new();
    let calls = Rc::new(Cell::new(0));
    let signal = SignalBuilder::from_scan(0, {
        let calls = calls.clone();
        let mut count = 0;
        move |value, _| {
            count += 1;
            calls.set(count);
            *value = count;
            if count == 1 {
                std::panic!("scan failed");
            }
        }
    })
    .on_discard(|_| ())
    .build();

    assert!(catch_unwind(AssertUnwindSafe(|| signal.get(&mut rt.sc()))).is_err());
    // A panic releases the data borrow without rolling back state or rerunning the scan.
    assert_eq!((signal.get(&mut rt.sc()), calls.get()), (1, 1));
    rt.flush();
    assert_eq!((signal.get(&mut rt.sc()), calls.get()), (2, 2));
}

#[test]
fn scan_map_preserves_unsized_borrows_across_reads() {
    let mut rt = Runtime::new();
    let source = State::new(String::from("hello"));
    let signal: Signal<str> = SignalBuilder::from_scan(String::new(), {
        let source = source.clone();
        move |value, sc| *value = source.get(sc)
    })
    .map_borrow::<str>()
    .map(|value| &value[1..])
    .build();

    {
        let mut sc = rt.sc();
        let first = signal.borrow(&mut sc);
        let second = signal.borrow(&mut sc);
        assert_eq!((&*first, &*second), ("ello", "ello"));
    }
    source.set(String::from("world"), rt.ac());
    assert_eq!(&*signal.borrow(&mut rt.sc()), "orld");
}

struct Observer<F>(F);

impl<F: Fn(DirtyLevel) + 'static> BindSink for Observer<F> {
    fn notify(self: Rc<Self>, _: Slot, level: DirtyLevel, _: &mut NotifyContext) {
        (self.0)(level);
    }
}

#[rstest::rstest]
fn notify_releases_data_before_notifying_dependants(#[values(false, true)] filter: bool) {
    let mut rt = Runtime::new();
    let node = ScanNode::new(
        0,
        Box::new(|state, _| {
            *state += 1;
            true
        }),
        filter,
        DiscardFnKeep,
        MapFnNone,
    );
    let notifications = Rc::new(RefCell::new(Vec::new()));
    let observer: Rc<dyn BindSink> = Rc::new(Observer({
        let node = node.clone();
        let notifications = notifications.clone();
        move |level| {
            // Reentrant access during downstream notification must succeed.
            notifications
                .borrow_mut()
                .push((level, node.data.borrow_mut().state));
        }
    }));
    let mut bindings = SourceBindings::new();
    bindings.update(
        Rc::downgrade(&observer),
        Slot(0),
        true,
        |sc| node.watch(sc),
        &mut rt.rc(),
    );
    node.clone()
        .notify(Slot(1), DirtyLevel::Dirty, rt.ac().nc());
    assert_eq!(*notifications.borrow(), vec![]);
    node.clone()
        .notify(Slot(0), DirtyLevel::Dirty, rt.ac().nc());
    node.clone()
        .notify(Slot(0), DirtyLevel::Dirty, rt.ac().nc());
    let level = if filter {
        DirtyLevel::MaybeDirty
    } else {
        DirtyLevel::Dirty
    };
    assert_eq!(*notifications.borrow(), vec![(level, 1)]);
    bindings.clear(&mut rt.rc());
    assert!(!rt.dispatch_discards());
}
