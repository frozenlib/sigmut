use super::*;
use pretty_assertions::assert_eq;

struct Source {
    id: usize,
    sinks: RefCell<SinkBindings>,
    unbound: Rc<RefCell<Vec<usize>>>,
}

impl Source {
    fn bind(self: &Rc<Self>, sc: &mut SignalContext<'_, '_>) {
        self.sinks.borrow_mut().bind(self.clone(), Slot(0), sc);
    }
}

impl BindSource for Source {
    fn check(self: Rc<Self>, _: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) -> bool {
        self.sinks.borrow().is_dirty(key, rc)
    }

    fn unbind(self: Rc<Self>, _: Slot, key: BindKey, rc: &mut ReactionContext<'_, '_>) {
        self.unbound.borrow_mut().push(self.id);
        self.sinks.borrow_mut().unbind(key, rc);
    }

    fn rebind(self: Rc<Self>, slot: Slot, key: BindKey, sc: &mut SignalContext<'_, '_>) {
        self.sinks.borrow_mut().rebind(self.clone(), slot, key, sc);
    }
}

struct Sink;

impl BindSink for Sink {
    fn notify(self: Rc<Self>, _: Slot, _: DirtyLevel, _: &mut NotifyContext) {}
}

#[test]
fn update_reuses_retained_dependencies_and_unbinds_only_the_suffix() {
    let mut rt = Runtime::new();
    let sink: Rc<dyn BindSink> = Rc::new(Sink);
    let unbound = Rc::new(RefCell::new(Vec::new()));
    let sources: [_; 3] = std::array::from_fn(|id| {
        Rc::new(Source {
            id,
            sinks: Default::default(),
            unbound: unbound.clone(),
        })
    });
    let mut bindings = SourceBindings::new();
    bindings.update(
        Rc::downgrade(&sink),
        Slot(0),
        true,
        |sc| {
            sources[0].bind(sc);
            sources[1].bind(sc);
        },
        &mut rt.rc(),
    );
    let original_keys: Vec<_> = bindings.0.iter().map(|b| b.key).collect();
    bindings.update(
        Rc::downgrade(&sink),
        Slot(0),
        false,
        |sc| sources[2].bind(sc),
        &mut rt.rc(),
    );
    assert_eq!(bindings.0.len(), 3);
    assert_eq!(
        bindings.0[..2].iter().map(|b| b.key).collect::<Vec<_>>(),
        original_keys
    );

    let value = String::from("borrowed return value");
    let result = bindings.update(
        Rc::downgrade(&sink),
        Slot(0),
        true,
        |sc| {
            sources[0].bind(sc);
            sources[1].bind(sc);
            value.as_str()
        },
        &mut rt.rc(),
    );
    assert_eq!(result, value);
    assert_eq!(*unbound.borrow(), vec![2]);
    assert_eq!(
        bindings.0.iter().map(|b| b.key).collect::<Vec<_>>(),
        original_keys
    );
    bindings.clear(&mut rt.rc());
    assert_eq!(*unbound.borrow(), vec![2, 0, 1]);
}

#[rstest::rstest]
fn callback_panic_defers_both_old_and_new_dependencies(#[values(false, true)] reset: bool) {
    let mut rt = Runtime::new();
    let sink: Rc<dyn BindSink> = Rc::new(Sink);
    let unbound = Rc::new(RefCell::new(Vec::new()));
    let sources: [_; 2] = std::array::from_fn(|id| {
        Rc::new(Source {
            id,
            sinks: Default::default(),
            unbound: unbound.clone(),
        })
    });
    let mut bindings = SourceBindings::new();
    bindings.update(
        Rc::downgrade(&sink),
        Slot(0),
        true,
        |sc| sources[0].bind(sc),
        &mut rt.rc(),
    );
    let result = catch_unwind(AssertUnwindSafe(|| {
        bindings.update(
            Rc::downgrade(&sink),
            Slot(0),
            reset,
            |sc| {
                if reset {
                    sources[0].bind(sc);
                }
                sources[1].bind(sc);
                std::panic!("callback failed");
            },
            &mut rt.rc(),
        );
    }));
    assert!(result.is_err());
    assert!(bindings.0.is_empty());
    assert_eq!(*unbound.borrow(), Vec::<usize>::new());
    rt.flush();
    assert_eq!(*unbound.borrow(), vec![0, 1]);
    assert!(sources.iter().all(|s| s.sinks.borrow().is_empty()));
}

#[test]
fn unbind_panic_drops_callback_result_before_the_update_sink() {
    struct PanickingSource;

    impl BindSource for PanickingSource {
        fn check(self: Rc<Self>, _: Slot, _: BindKey, _: &mut ReactionContext<'_, '_>) -> bool {
            false
        }
        fn unbind(self: Rc<Self>, _: Slot, _: BindKey, _: &mut ReactionContext<'_, '_>) {
            std::panic!("unbind failed");
        }
        fn rebind(self: Rc<Self>, _: Slot, _: BindKey, _: &mut SignalContext<'_, '_>) {
            unreachable!();
        }
    }

    struct ResultOnDrop {
        sink: Rc<dyn BindSink>,
        weak_count: Rc<Cell<usize>>,
    }

    impl Drop for ResultOnDrop {
        fn drop(&mut self) {
            self.weak_count.set(Rc::weak_count(&self.sink));
        }
    }

    let mut rt = Runtime::new();
    let sink: Rc<dyn BindSink> = Rc::new(Sink);
    let weak_count = Rc::new(Cell::new(usize::MAX));
    let mut bindings = SourceBindings(vec![SourceBinding {
        source: Rc::new(PanickingSource),
        slot: Slot(0),
        key: BindKey(0),
    }]);
    let result = catch_unwind(AssertUnwindSafe(|| {
        bindings.update(
            Rc::downgrade(&sink),
            Slot(0),
            true,
            |_| ResultOnDrop {
                sink: sink.clone(),
                weak_count: weak_count.clone(),
            },
            &mut rt.rc(),
        )
    }));
    assert!(result.is_err());
    assert_eq!(weak_count.get(), 1);
    assert_eq!(Rc::weak_count(&sink), 0);
    assert!(bindings.0.is_empty());
}
