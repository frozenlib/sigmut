#![allow(clippy::items_after_test_module)]

use std::{cell::Cell, rc::Rc};

use reactive_fn::core::{ComputeContext, Runtime};
use reactive_fn::helpers::dependency_node::{Compute, DependencyNode, DependencyNodeSettings};
use rstest::rstest;

use crate::test_utils::code_path::{code, CodePath, CodePathChecker};

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
    let mut cp = CodePathChecker::new();
    let mut _rt = Runtime::new();
    let _node = Node::new(0, |_| true, false, false, true);

    // Not immediately computed.
    cp.verify();
}

#[test]
fn watch() {
    let mut cp = CodePathChecker::new();
    let mut dc = Runtime::new();
    let id = 0;
    let node = Node::new(id, |_| true, false, false, true);

    // Computed when `watch` is called.
    node.watch(dc.ac().oc());
    cp.expect(compute(id));
    cp.verify();
}

#[test]
fn watch_2() {
    let mut cp = CodePathChecker::new();
    let mut dc = Runtime::new();
    let id = 0;
    let node = Node::new(id, |_| true, false, false, true);

    // Multiple calls to `watch` are computed only once.
    node.watch(dc.ac().oc());
    node.watch(dc.ac().oc());
    cp.expect(compute(id));
    cp.verify();
}

#[test]
fn watch_notify() {
    let mut cp = CodePathChecker::new();
    let mut dc = Runtime::new();
    let id = 0;
    let node = Node::new(id, |_| true, false, false, true);

    node.watch(dc.ac().oc());
    cp.expect(compute(id));
    cp.verify();

    node.notify(&mut dc.ac());
    cp.verify();

    // After calling `notify`, the node is recomputed when `ObsContext` is retrieved.
    node.watch(dc.ac().oc());
    cp.expect(compute(id));
    cp.verify();
}

#[test]
fn new_node_watch_in_compute() {
    let mut dc = Runtime::new();
    let _node = Node::new(
        0,
        |cc| {
            let node2 = Node::new(1, |_| true, false, false, true);
            node2.watch(cc.oc());
            true
        },
        false,
        true,
        true,
    );
    dc.update();
}

#[test]
fn new_node_is_up_to_date_in_compute() {
    let mut dc = Runtime::new();
    let _node = Node::new(
        0,
        |mut cc| {
            let node2 = Node::new(1, |_| true, false, false, true);
            node2.is_up_to_date(cc.uc());
            true
        },
        false,
        true,
        true,
    );
    dc.update();
}

#[test]
fn is_hot_false() {
    let mut cp = CodePathChecker::new();
    let mut dc = Runtime::new();
    let id = 0;

    // If is_hot is false, it is not computed when `update` is called.
    let _node = Node::new(id, |_| true, false, false, true);
    dc.update();
    cp.verify();
}

#[test]
fn is_hot_true() {
    let mut cp = CodePathChecker::new();
    let mut dc = Runtime::new();
    let id = 0;

    // If is_hot is true, it is computed when `update` is called.
    let _node = Node::new(id, |_| true, false, true, true);
    dc.update();
    cp.expect(compute(id));
    cp.verify();
}

#[test]
fn is_hot_false_discard() {
    let mut cp = CodePathChecker::new();
    let mut dc = Runtime::new();
    let id = 0;
    let node = Node::new(id, |_| true, false, false, true);
    node.watch(dc.ac().oc());
    cp.expect([compute(id)]);
    cp.verify();

    dc.update();
    cp.expect([discard(id)]);
    cp.verify();
}

#[rstest]
fn dependencies(
    #[values(1, 2, 3, 4)] count: usize,
    #[values(false, true)] is_modify_always_this: bool,
    #[values(false, true)] is_modify_always_deps: bool,
) {
    let mut cp = CodePathChecker::new();
    let mut dc = Runtime::new();
    let id_this = usize::MAX;
    let mut deps = Vec::new();
    for i in 0..count {
        deps.push(Node::new(i, |_| true, false, false, is_modify_always_deps));
    }
    let this = Node::new(
        id_this,
        {
            let deps = deps.clone();
            move |cc| {
                let oc = cc.oc();
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
    this.watch(dc.ac().oc());
    cp.expect([compute(id_this)]);
    for i in 0..count {
        cp.expect([compute(i)]);
    }
    cp.verify();

    (0..count).for_each(|i| {
        deps[i].notify(&mut dc.ac());
        dc.update_with(false);
        if is_modify_always_deps {
            cp.expect([compute(id_this), compute(i)]);
        } else {
            cp.expect([compute(i), compute(id_this)]);
        }
        cp.verify();
    });
}

#[rstest]
fn dependants(
    #[values(1, 2, 3, 4)] count: usize,
    #[values(false, true)] is_hot_this: bool,
    #[values(false, true)] is_modify_always_this: bool,
    #[values(false, true)] is_modify_always_deps: bool,
) {
    let mut cp = CodePathChecker::new();
    let mut dc = Runtime::new();
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

    let mut all: Vec<CodePath> = (0..count).map(compute).collect();
    all.push(compute(id_this));

    dc.update_with(false);
    cp.expect_set(all.clone());
    cp.verify();

    this.notify(&mut dc.ac());
    dc.update_with(false);
    cp.expect_set(all.clone());
    cp.verify();

    (0..count).for_each(|id| {
        deps[id].notify(&mut dc.ac());
        dc.ac().oc();

        dc.update_with(false);
        cp.expect(compute(id));
        cp.verify_msg(&format!("id = {id}"));
    });
}

#[test]
fn change_dependency() {
    let mut cp = CodePathChecker::new();
    let mut dc = Runtime::new();
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
            move |cc| {
                deps[d.get()].watch(cc.oc());
                true
            }
        },
        false,
        true,
        false,
    );

    dc.update();
    cp.expect([compute(id_this), compute(0)]);
    cp.verify();

    deps[1].notify(&mut dc.ac());
    dc.update();
    cp.verify();

    deps[0].notify(&mut dc.ac());
    dc.update();
    cp.expect([compute(id_this), compute(0)]);
    cp.verify();

    d.set(1);
    this.notify(&mut dc.ac());
    dc.update();
    cp.expect([compute(id_this), compute(1), discard(0)]);
    cp.verify();

    deps[0].notify(&mut dc.ac());
    dc.update();
    cp.verify();

    deps[1].notify(&mut dc.ac());
    dc.update();
    cp.expect([compute(id_this), compute(1)]);
    cp.verify();
}

#[test]
fn is_modify_always_false() {
    let mut cp = CodePathChecker::new();
    let mut dc = Runtime::new();
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

    dc.update();
    cp.expect([compute(dep), compute(src)]);
    cp.verify();

    // If the source is not modified, dependants will not recompute.
    is_modified.set(false);
    src_node.notify(&mut dc.ac());
    dc.update();
    cp.expect([compute(src)]);
    cp.verify();

    // If the source is modified, dependants will recompute.
    is_modified.set(true);
    src_node.notify(&mut dc.ac());
    dc.update();
    cp.expect([compute(src), compute(dep)]);
    cp.verify();
}

#[rstest]
fn is_modify_always_true(#[values(false, true)] ret_is_modified: bool) {
    let mut cp = CodePathChecker::new();
    let mut dc = Runtime::new();
    let src = 0;
    let dep = 1;
    let src_node = Node::new(src, move |_| ret_is_modified, false, false, true);
    let _dep_node = Node::new(dep, compute_depend_on(&src_node), false, true, true);

    dc.update();
    cp.expect([compute(dep), compute(src)]);
    cp.verify();

    // Since source is always modified, it is not recomputed to check if it has been modified or not.
    // Therefore, the compute is done after the request from the dependants.
    src_node.notify(&mut dc.ac());
    dc.update();
    cp.expect([compute(dep), compute(src)]);
    cp.verify();
}

#[test]
fn is_modify_always_false_true_true() {
    let mut cp = CodePathChecker::new();
    let mut dc = Runtime::new();
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

    dc.update();
    cp.expect([compute(2), compute(1), compute(0)]);
    cp.verify();

    // If the source is not modified, dependants will not recompute.
    is_modified.set(false);
    node0.notify(&mut dc.ac());
    dc.update();
    cp.expect([compute(0)]);
    cp.verify();

    // If the source is modified, dependants will recompute.
    is_modified.set(true);
    node0.notify(&mut dc.ac());
    dc.update();
    cp.expect([compute(0), compute(2), compute(1)]);
    cp.verify();
}

#[test]
fn is_modify_always_false_true_false() {
    let mut cp = CodePathChecker::new();
    let mut dc = Runtime::new();
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

    dc.update();
    cp.expect([compute(2), compute(1), compute(0)]);
    cp.verify();

    // If the source is not modified, dependants will not recompute.
    // Nodes where is_modify_always is true are not precomputed.
    is_modified.set(false);
    node0.notify(&mut dc.ac());
    dc.update();
    cp.expect([compute(0)]);
    cp.verify();

    // If the source is modified, dependants will recompute.
    // Nodes where is_modify_always is true are not precomputed.
    is_modified.set(true);
    node0.notify(&mut dc.ac());
    dc.update();
    cp.expect([compute(0), compute(2), compute(1)]);
    cp.verify();
}

#[test]
fn is_hasty_true() {
    let mut cp = CodePathChecker::new();
    let mut dc = Runtime::new();
    let node0 = Node::new(0, |_| true, true, false, false);

    // Not recomputed if there is no dependant node.
    dc.update();
    cp.verify();

    let node1 = Node::new(1, compute_depend_on(&node0), false, true, false);

    dc.update();
    cp.expect([compute(1), compute(0)]);
    cp.verify();

    // Nodes where `is_hasty` is true are recomputed first.
    node0.notify(&mut dc.ac());
    dc.update();
    cp.expect([compute(0), compute(1)]);
    cp.verify();

    // Not recomputed if there is no dependant node.
    node0.notify(&mut dc.ac());
    drop(node1);
    cp.expect([discard(0)]);
    dc.update();
    cp.verify();
}

#[test]
fn is_hasty_true_is_modify_always_true() {
    let mut cp = CodePathChecker::new();
    let mut dc = Runtime::new();
    let node0 = Node::new(0, |_| true, false, false, false);
    let node1 = Node::new(1, compute_depend_on(&node0), true, false, true);
    let _node2 = Node::new(2, compute_depend_on(&node1), false, true, false);

    dc.update();
    cp.expect([compute(2), compute(1), compute(0)]);

    // Nodes with is_hasty true are determined if they have been updated before other nodes.
    // If is_modify_always is true, it is not necessary to compute for this purpose, so its own computation is not performed first.
    node0.notify(&mut dc.ac());
    dc.update();
    cp.expect([compute(0), compute(2), compute(1)]);
    cp.verify();
}

#[test]
fn is_hasty_true_is_modify_always_false() {
    let mut cp = CodePathChecker::new();
    let mut dc = Runtime::new();
    let node0 = Node::new(0, |_| true, false, false, false);
    let node1 = Node::new(1, compute_depend_on(&node0), true, false, false);
    let _node2 = Node::new(2, compute_depend_on(&node1), false, true, false);

    dc.update();
    cp.expect([compute(2), compute(1), compute(0)]);
    cp.verify();

    node0.notify(&mut dc.ac());
    dc.update();
    cp.expect([compute(0), compute(1), compute(2)]);
    cp.verify();
}

#[test]
fn is_hasty_true_is_hot_true() {
    let mut cp = CodePathChecker::new();
    let mut dc = Runtime::new();
    let node0 = Node::new(0, |_| true, true, true, false);
    let node1 = Node::new(1, |_| true, true, false, false);
    let _node2 = Node::new(2, compute_depend_on(&node1), false, true, false);

    dc.update();
    cp.expect_set([compute(0), compute(1), compute(2)]);
    cp.verify();

    // If is_hasty is true and there is no dependent node, it is computed with normal priority.
    node0.notify(&mut dc.ac());
    node1.notify(&mut dc.ac());
    dc.update();
    cp.expect([compute(1)]);
    cp.expect_set([compute(0), compute(2)]);
    cp.verify();
}

#[test]
fn stop_recompute_when_one_of_dependencies_is_modified() {
    let mut cp = CodePathChecker::new();
    let mut dc = Runtime::new();
    let id_this = usize::MAX;
    let dep0 = Node::new(0, move |_| true, false, false, false);
    let dep1 = Node::new(1, move |_| true, false, false, false);

    let _this = Node::new(
        id_this,
        {
            let dep0 = dep0.clone();
            let dep1 = dep1.clone();
            move |cc| {
                let oc = cc.oc();
                dep0.watch(oc);
                dep1.watch(oc);
                true
            }
        },
        false,
        true,
        false,
    );
    dc.update();
    cp.expect([compute(id_this), compute(0), compute(1)]);

    dep0.notify(&mut dc.ac());
    dep1.notify(&mut dc.ac());
    dc.update();
    // Not `0,1,this` or `1,0,this` , but as follows
    cp.expect_any([
        [compute(0), compute(id_this), compute(1)],
        [compute(1), compute(id_this), compute(0)],
    ]);
    cp.verify();
}

#[rstest]
fn dependency_hold_strong_ref(#[values(false, true)] is_hot: bool) {
    let mut dc = Runtime::new();
    let node0 = Node::new(0, move |_| true, false, is_hot, false);
    let node0_w = Rc::downgrade(&node0);
    let node1 = Node::new(
        1,
        {
            let node0_w = node0_w.clone();
            move |cc| {
                if let Some(node0_w) = node0_w.upgrade() {
                    node0_w.watch(cc.oc());
                }
                true
            }
        },
        false,
        true,
        false,
    );
    dc.update();
    drop(node0);
    dc.update();
    assert!(node0_w.upgrade().is_some());

    drop(node1);
    dc.update();
    assert!(node0_w.upgrade().is_none());
}

// 旧依存継承
// Wakerによる更新通知
// 循環参照
// 菱形参照

#[test]
fn is_up_to_date() {
    let mut dc = Runtime::new();
    let node = Node::new(0, |_| true, false, false, true);
    assert!(!node.is_up_to_date(dc.uc()));

    node.watch(dc.ac().oc());
    assert!(node.is_up_to_date(dc.uc()));

    node.notify(&mut dc.ac());
    assert!(!node.is_up_to_date(dc.uc()));

    node.watch(dc.ac().oc());
    assert!(node.is_up_to_date(dc.uc()));
}

#[test]
fn is_up_to_date_dependant() {
    let mut dc = Runtime::new();
    let node0 = Node::new(0, |_| true, false, false, true);
    let node1 = Node::new(1, compute_depend_on(&node0), false, true, true);
    assert!(!node0.is_up_to_date(dc.uc()));
    assert!(!node1.is_up_to_date(dc.uc()));
    node1.watch(dc.ac().oc());

    assert!(node0.is_up_to_date(dc.uc()));
    assert!(node1.is_up_to_date(dc.uc()));

    node0.notify(&mut dc.ac());
    assert!(!node0.is_up_to_date(dc.uc()));
    assert!(!node1.is_up_to_date(dc.uc()));
}

#[test]
fn is_hot_and_dependency() {
    let mut dc = Runtime::new();
    let node0 = Node::new(0, |_| true, false, false, false);
    let node1 = Node::new(0, compute_depend_on(&node0), false, true, false);
    dc.update();
    assert!(node0.is_up_to_date(dc.uc()));
    assert!(node1.is_up_to_date(dc.uc()));

    node0.notify(&mut dc.ac());
    assert!(!node0.is_up_to_date(dc.uc()));
    assert!(!node1.is_up_to_date(dc.uc()));
}

#[test]
fn watch_in_borrow() {
    let mut dc = Runtime::new();
    let node0 = Node::new(0, |_| true, false, false, false);
    node0.watch(dc.ac().oc());
    let b = node0.borrow();
    node0.watch(dc.ac().oc());
    drop(b);
}

#[allow(clippy::type_complexity)]
struct Node {
    id: usize,
    compute: Box<dyn FnMut(ComputeContext) -> bool + 'static>,
}

impl Node {
    fn new(
        id: usize,
        compute: impl FnMut(ComputeContext) -> bool + 'static,
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
    fn compute(&mut self, cc: ComputeContext) -> bool {
        code(format!("compute {}", self.id));
        (self.compute)(cc)
    }

    fn discard(&mut self) -> bool {
        code(format!("discard {}", self.id));
        false
    }
}

fn compute_depend_on(node: &Rc<DependencyNode<Node>>) -> impl Fn(ComputeContext) -> bool {
    let node = node.clone();
    move |cc: ComputeContext| {
        node.watch(cc.oc());
        true
    }
}

fn compute(x: usize) -> CodePath {
    CodePath::new(format!("compute {x}"))
}
fn discard(x: usize) -> CodePath {
    CodePath::new(format!("discard {x}"))
}
