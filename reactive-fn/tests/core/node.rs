#![allow(clippy::items_after_test_module)]

use std::{cell::Cell, rc::Rc};

use assert_call::{call, Call, CallRecorder};
use reactive_fn::core::Runtime;
use reactive_fn::helpers::dependency_node::{Compute, DependencyNode, DependencyNodeSettings};
use reactive_fn::ObsContext;
use rstest::rstest;

#[test]
fn runtime() {
    let mut _rt = Runtime::new();
}

#[test]
fn new_node() {
    let mut _rt = Runtime::new();
    let _ = Node::new(0, |_| true, false, false, false);
}

#[test]
fn new_node_2() {
    let mut _rt = Runtime::new();
    let _ = Node::new(0, |_| true, false, false, false);
    let _ = Node::new(0, |_| true, false, false, false);
}

#[test]
fn new_with_owner() {
    let mut c = CallRecorder::new();
    let mut _rt = Runtime::new();
    let _node = Node::new(0, |_| true, false, false, true);

    // Not immediately computed.
    c.verify(());
}

#[test]
fn watch() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let id = 0;
    let node = Node::new(id, |_| true, false, false, true);

    // Computed when `watch` is called.
    node.watch(&mut rt.oc());
    c.verify(compute(id));
}

#[test]
fn watch_2() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let id = 0;
    let node = Node::new(id, |_| true, false, false, true);

    // Multiple calls to `watch` are computed only once.
    node.watch(&mut rt.oc());
    node.watch(&mut rt.oc());
    c.verify(compute(id));
}

#[test]
fn watch_notify() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let id = 0;
    let node = Node::new(id, |_| true, false, false, true);

    node.watch(&mut rt.oc());
    c.verify(compute(id));

    node.notify(&mut rt.ac());
    c.verify(());

    // After calling `notify`, the node is recomputed when `ObsContext` is retrieved.
    node.watch(&mut rt.oc());
    c.verify(compute(id));
}

#[test]
fn new_node_watch_in_compute() {
    let mut rt = Runtime::new();
    let _node = Node::new(
        0,
        |oc| {
            let node2 = Node::new(1, |_| true, false, false, true);
            node2.watch(oc.reset());
            true
        },
        false,
        true,
        true,
    );
    rt.update();
}

#[test]
fn new_node_is_up_to_date_in_compute() {
    let mut rt = Runtime::new();
    let _node = Node::new(
        0,
        |oc| {
            let node2 = Node::new(1, |_| true, false, false, true);
            node2.is_up_to_date(oc.uc());
            true
        },
        false,
        true,
        true,
    );
    rt.update();
}

#[test]
fn is_hot_false() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let id = 0;

    // If is_hot is false, it is not computed when `update` is called.
    let _node = Node::new(id, |_| true, false, false, true);
    rt.update();
    c.verify(());
}

#[test]
fn is_hot_true() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let id = 0;

    // If is_hot is true, it is computed when `update` is called.
    let _node = Node::new(id, |_| true, false, true, true);
    rt.update();
    c.verify(compute(id));
}

#[test]
fn is_hot_false_discard() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let id = 0;
    let node = Node::new(id, |_| true, false, false, true);
    node.watch(&mut rt.oc());
    c.verify([compute(id)]);

    rt.update();
    c.verify([discard(id)]);
}

#[rstest]
fn dependencies(
    #[values(1, 2, 3, 4)] count: usize,
    #[values(false, true)] is_modify_always_this: bool,
    #[values(false, true)] is_modify_always_deps: bool,
) {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let id_this = usize::MAX;
    let mut deps = Vec::new();
    for i in 0..count {
        deps.push(Node::new(i, |_| true, false, false, is_modify_always_deps));
    }
    let this = Node::new(
        id_this,
        {
            let deps = deps.clone();
            move |oc| {
                oc.reset();
                for dep in &deps {
                    dep.watch(oc);
                }
                true
            }
        },
        false,
        true,
        is_modify_always_this,
    );
    this.watch(&mut rt.oc());

    let mut cs = Vec::new();
    cs.push(compute(id_this));
    for i in 0..count {
        cs.push(compute(i));
    }
    c.verify(cs);

    (0..count).for_each(|i| {
        deps[i].notify(&mut rt.ac());
        rt.update_with(false);
        if is_modify_always_deps {
            c.verify([compute(id_this), compute(i)]);
        } else {
            c.verify([compute(i), compute(id_this)]);
        }
    });
}

#[rstest]
fn dependants(
    #[values(1, 2, 3, 4)] count: usize,
    #[values(false, true)] is_hot_this: bool,
    #[values(false, true)] is_modify_always_this: bool,
    #[values(false, true)] is_modify_always_deps: bool,
) {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let id_this = usize::MAX;
    let mut deps = Vec::new();
    let this = Node::new(id_this, |_| true, false, is_hot_this, is_modify_always_this);
    for id in 0..count {
        deps.push(Node::new(
            id,
            compute_depend_on(&this),
            false,
            true,
            is_modify_always_deps,
        ));
    }

    let mut all: Vec<Call> = (0..count).map(compute).collect();
    all.push(compute(id_this));

    rt.update_with(false);
    c.verify(Call::par(&all));

    this.notify(&mut rt.ac());
    rt.update_with(false);
    c.verify(Call::par(&all));

    (0..count).for_each(|id| {
        deps[id].notify(&mut rt.ac());

        rt.update_with(false);
        c.verify_with_msg(compute(id), &format!("id = {id}"));
    });
}

#[test]
fn change_dependency() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let mut deps = Vec::new();
    for id in 0..=1 {
        deps.push(Node::new(id, |_| true, false, false, true));
    }
    let d = Rc::new(Cell::new(0));
    let id_this = usize::MAX;

    let this = Node::new(
        id_this,
        {
            let d = d.clone();
            let deps = deps.clone();
            move |oc| {
                deps[d.get()].watch(oc.reset());
                true
            }
        },
        false,
        true,
        false,
    );

    rt.update();
    c.verify([compute(id_this), compute(0)]);

    deps[1].notify(&mut rt.ac());
    rt.update();
    c.verify(());

    deps[0].notify(&mut rt.ac());
    rt.update();
    c.verify([compute(id_this), compute(0)]);

    d.set(1);
    this.notify(&mut rt.ac());
    rt.update();
    c.verify([compute(id_this), compute(1), discard(0)]);

    deps[0].notify(&mut rt.ac());
    rt.update();
    c.verify(());

    deps[1].notify(&mut rt.ac());
    rt.update();
    c.verify([compute(id_this), compute(1)]);
}

#[test]
fn is_modify_always_false() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let src = 0;
    let dep = 1;
    let is_modified = Rc::new(Cell::default());
    let src_node = Node::new(
        src,
        {
            let is_modified = is_modified.clone();
            move |_| is_modified.get()
        },
        false,
        false,
        false,
    );
    let _dep_node = Node::new(dep, compute_depend_on(&src_node), false, true, true);

    rt.update();
    c.verify([compute(dep), compute(src)]);
    // If the source is not modified, dependants will not recompute.
    is_modified.set(false);
    src_node.notify(&mut rt.ac());
    rt.update();
    c.verify([compute(src)]);
    // If the source is modified, dependants will recompute.
    is_modified.set(true);
    src_node.notify(&mut rt.ac());
    rt.update();
    c.verify([compute(src), compute(dep)]);
}

#[rstest]
fn is_modify_always_true(#[values(false, true)] ret_is_modified: bool) {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let src = 0;
    let dep = 1;
    let src_node = Node::new(src, move |_| ret_is_modified, false, false, true);
    let _dep_node = Node::new(dep, compute_depend_on(&src_node), false, true, true);

    rt.update();
    c.verify([compute(dep), compute(src)]);
    // Since source is always modified, it is not recomputed to check if it has been modified or not.
    // Therefore, the compute is done after the request from the dependants.
    src_node.notify(&mut rt.ac());
    rt.update();
    c.verify([compute(dep), compute(src)]);
}

#[test]
fn is_modify_always_false_true_true() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let is_modified = Rc::new(Cell::default());
    let node0 = Node::new(
        0,
        {
            let is_modified = is_modified.clone();
            move |_| is_modified.get()
        },
        false,
        false,
        false,
    );
    let node1 = Node::new(1, compute_depend_on(&node0), false, false, true);
    let _node2 = Node::new(2, compute_depend_on(&node1), false, true, true);

    rt.update();
    c.verify([compute(2), compute(1), compute(0)]);
    // If the source is not modified, dependants will not recompute.
    is_modified.set(false);
    node0.notify(&mut rt.ac());
    rt.update();
    c.verify([compute(0)]);
    // If the source is modified, dependants will recompute.
    is_modified.set(true);
    node0.notify(&mut rt.ac());
    rt.update();
    c.verify([compute(0), compute(2), compute(1)]);
}

#[test]
fn is_modify_always_false_true_false() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let is_modified = Rc::new(Cell::default());
    let node0 = Node::new(
        0,
        {
            let is_modified = is_modified.clone();
            move |_| is_modified.get()
        },
        false,
        false,
        false,
    );
    let node1 = Node::new(1, compute_depend_on(&node0), false, false, true);
    let _node2 = Node::new(2, compute_depend_on(&node1), false, true, false);

    rt.update();
    c.verify([compute(2), compute(1), compute(0)]);
    // If the source is not modified, dependants will not recompute.
    // Nodes where is_modify_always is true are not precomputed.
    is_modified.set(false);
    node0.notify(&mut rt.ac());
    rt.update();
    c.verify([compute(0)]);
    // If the source is modified, dependants will recompute.
    // Nodes where is_modify_always is true are not precomputed.
    is_modified.set(true);
    node0.notify(&mut rt.ac());
    rt.update();
    c.verify([compute(0), compute(2), compute(1)]);
}

#[test]
fn is_hasty_true() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let node0 = Node::new(0, |_| true, true, false, false);

    // Not recomputed if there is no dependant node.
    rt.update();
    c.verify(());
    let node1 = Node::new(1, compute_depend_on(&node0), false, true, false);

    rt.update();
    c.verify([compute(1), compute(0)]);
    // Nodes where `is_hasty` is true are recomputed first.
    node0.notify(&mut rt.ac());
    rt.update();
    c.verify([compute(0), compute(1)]);
    // Not recomputed if there is no dependant node.
    node0.notify(&mut rt.ac());
    drop(node1);
    rt.update();
    c.verify([discard(0)]);
}

#[test]
fn is_hasty_true_is_modify_always_true() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let node0 = Node::new(0, |_| true, false, false, false);
    let node1 = Node::new(1, compute_depend_on(&node0), true, false, true);
    let _node2 = Node::new(2, compute_depend_on(&node1), false, true, false);

    rt.update();
    c.verify([compute(2), compute(1), compute(0)]);

    // Nodes with is_hasty true are determined if they have been updated before other nodes.
    // If is_modify_always is true, it is not necessary to compute for this purpose, so its own computation is not performed first.
    node0.notify(&mut rt.ac());
    rt.update();
    c.verify([compute(0), compute(2), compute(1)]);
}

#[test]
fn is_hasty_true_is_modify_always_false() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let node0 = Node::new(0, |_| true, false, false, false);
    let node1 = Node::new(1, compute_depend_on(&node0), true, false, false);
    let _node2 = Node::new(2, compute_depend_on(&node1), false, true, false);

    rt.update();
    c.verify([compute(2), compute(1), compute(0)]);
    node0.notify(&mut rt.ac());
    rt.update();
    c.verify([compute(0), compute(1), compute(2)]);
}

#[test]
fn is_hasty_true_is_hot_true() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let node0 = Node::new(0, |_| true, true, true, false);
    let node1 = Node::new(1, |_| true, true, false, false);
    let _node2 = Node::new(2, compute_depend_on(&node1), false, true, false);

    rt.update();
    c.verify(Call::par([compute(0), compute(1), compute(2)]));
    // If is_hasty is true and there is no dependent node, it is computed with normal priority.
    node0.notify(&mut rt.ac());
    node1.notify(&mut rt.ac());
    rt.update();
    c.verify([compute(1), Call::par([compute(0), compute(2)])]);
}

#[test]
fn stop_recompute_when_one_of_dependencies_is_modified() {
    let mut c = CallRecorder::new();
    let mut rt = Runtime::new();
    let id_this = usize::MAX;
    let dep0 = Node::new(0, move |_| true, false, false, false);
    let dep1 = Node::new(1, move |_| true, false, false, false);

    let _this = Node::new(
        id_this,
        {
            let dep0 = dep0.clone();
            let dep1 = dep1.clone();
            move |oc| {
                oc.reset();
                dep0.watch(oc);
                dep1.watch(oc);
                true
            }
        },
        false,
        true,
        false,
    );
    rt.update();
    c.verify([compute(id_this), compute(0), compute(1)]);

    dep0.notify(&mut rt.ac());
    dep1.notify(&mut rt.ac());
    rt.update();
    // Not `0,1,this` or `1,0,this` , but as follows
    c.verify(Call::any([
        [compute(0), compute(id_this), compute(1)],
        [compute(1), compute(id_this), compute(0)],
    ]));
}

#[rstest]
fn dependency_hold_strong_ref(#[values(false, true)] is_hot: bool) {
    let mut rt = Runtime::new();
    let node0 = Node::new(0, move |_| true, false, is_hot, false);
    let node0_w = Rc::downgrade(&node0);
    let node1 = Node::new(
        1,
        {
            let node0_w = node0_w.clone();
            move |oc| {
                oc.reset();
                if let Some(node0_w) = node0_w.upgrade() {
                    node0_w.watch(oc);
                }
                true
            }
        },
        false,
        true,
        false,
    );
    rt.update();
    drop(node0);
    rt.update();
    assert!(node0_w.upgrade().is_some());

    drop(node1);
    rt.update();
    assert!(node0_w.upgrade().is_none());
}

// 旧依存継承
// Wakerによる更新通知
// 循環参照
// 菱形参照

#[test]
fn is_up_to_date() {
    let mut rt = Runtime::new();
    let node = Node::new(0, |_| true, false, false, true);
    assert!(!node.is_up_to_date(&mut rt.uc()));

    node.watch(&mut rt.oc());
    assert!(node.is_up_to_date(&mut rt.uc()));

    node.notify(&mut rt.ac());
    assert!(!node.is_up_to_date(&mut rt.uc()));

    node.watch(&mut rt.oc());
    assert!(node.is_up_to_date(&mut rt.uc()));
}

#[test]
fn is_up_to_date_dependant() {
    let mut rt = Runtime::new();
    let node0 = Node::new(0, |_| true, false, false, true);
    let node1 = Node::new(1, compute_depend_on(&node0), false, true, true);
    assert!(!node0.is_up_to_date(&mut rt.uc()));
    assert!(!node1.is_up_to_date(&mut rt.uc()));
    node1.watch(&mut rt.oc());

    assert!(node0.is_up_to_date(&mut rt.uc()));
    assert!(node1.is_up_to_date(&mut rt.uc()));

    node0.notify(&mut rt.ac());
    assert!(!node0.is_up_to_date(&mut rt.uc()));
    assert!(!node1.is_up_to_date(&mut rt.uc()));
}

#[test]
fn is_hot_and_dependency() {
    let mut rt = Runtime::new();
    let node0 = Node::new(0, |_| true, false, false, false);
    let node1 = Node::new(0, compute_depend_on(&node0), false, true, false);
    rt.update();
    assert!(node0.is_up_to_date(&mut rt.uc()));
    assert!(node1.is_up_to_date(&mut rt.uc()));

    node0.notify(&mut rt.ac());
    assert!(!node0.is_up_to_date(&mut rt.uc()));
    assert!(!node1.is_up_to_date(&mut rt.uc()));
}

#[test]
fn watch_in_borrow() {
    let mut rt = Runtime::new();
    let node0 = Node::new(0, |_| true, false, false, false);
    node0.watch(&mut rt.oc());
    let b = node0.borrow();
    node0.watch(&mut rt.oc());
    drop(b);
}

#[allow(clippy::type_complexity)]
struct Node {
    id: usize,
    compute: Box<dyn FnMut(&mut ObsContext) -> bool + 'static>,
}

impl Node {
    fn new(
        id: usize,
        compute: impl FnMut(&mut ObsContext) -> bool + 'static,
        is_hasty: bool,
        is_hot: bool,
        is_modify_always: bool,
    ) -> Rc<DependencyNode<Node>> {
        DependencyNode::new(
            Node {
                id,
                compute: Box::new(compute),
            },
            DependencyNodeSettings {
                is_hasty,
                is_hot,
                is_modify_always,
            },
        )
    }
}

impl Compute for Node {
    fn compute(&mut self, oc: &mut ObsContext) -> bool {
        call!("compute {}", self.id);
        (self.compute)(oc)
    }

    fn discard(&mut self) -> bool {
        call!("discard {}", self.id);
        false
    }
}

fn compute_depend_on(node: &Rc<DependencyNode<Node>>) -> impl Fn(&mut ObsContext) -> bool {
    let node = node.clone();
    move |oc: &mut ObsContext| {
        node.watch(oc.reset());
        true
    }
}

fn compute(x: usize) -> Call {
    Call::id(format!("compute {x}"))
}
fn discard(x: usize) -> Call {
    Call::id(format!("discard {x}"))
}
