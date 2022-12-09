use reactive_fn::{ObsContext, ObsListCell};
use rt_local::runtime::core::test;

#[test]
async fn as_dyn() {
    let cell = ObsListCell::new();
    cell.borrow_mut().push(0);
    let o = cell.as_dyn();
    ObsContext::null(|bc| {
        let b = o.borrow(bc);
        assert_eq!(b.len(), 1);
    });
}
