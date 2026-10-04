use super::*;
use crate::core::{Runtime, SourceBindings};
use pretty_assertions::assert_eq;

struct Observer<F>(F);

impl<F: Fn(DirtyLevel) + 'static> BindSink for Observer<F> {
    fn notify(self: Rc<Self>, _: Slot, level: DirtyLevel, _: &mut NotifyContext) {
        (self.0)(level);
    }
}

#[test]
fn notify_releases_data_before_notifying_dependants() {
    let mut rt = Runtime::new();
    let node = Rc::new_cyclic(|this| ScanNode {
        bindings: ScanBindings {
            sinks: Default::default(),
        },
        data: RefCell::new(ScanNodeData {
            state: 0,
            scan: ScanFnBool(|state: &mut i32, _: &mut SignalContext<'_, '_>| {
                *state += 1;
                true
            }),
            sb: SourceBinder::new(this, Slot(0)),
        }),
        discard: DiscardFnKeep,
        discard_scheduled: (),
        map: MapFnNone,
    });
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
    assert_eq!(*notifications.borrow(), vec![(DirtyLevel::MaybeDirty, 1)]);
    bindings.clear(&mut rt.rc());
    assert!(!rt.dispatch_discards());
}
