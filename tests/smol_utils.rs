use futures::{
    future::{select, Either},
    stream::StreamExt,
    Future,
};
use reactive_fn::smol_utils::*;
use reactive_fn::*;
use smol::{Task, Timer};
use std::time::Duration;

async fn assert_timeout(fut: impl Future + Unpin) {
    let dur = Duration::from_millis(600);
    let res = select(fut, Timer::after(dur)).await;
    assert!(matches!(res, Either::Right(_)), "not timeouted");
}

#[test]
fn re_to_stream() {
    smol::run(async {
        let cell = ReCell::new(1u64);

        let mut s = cell.to_re().to_stream();
        let dur = Duration::from_millis(300);
        let _task = Task::local(async move {
            Timer::after(dur).await;
            cell.set_and_update(5);
            Timer::after(dur).await;
            cell.set_and_update(6);
        });
        assert_eq!(s.next().await, Some(1));
        assert_eq!(s.next().await, Some(5));
        assert_eq!(s.next().await, Some(6));
        assert_timeout(s.next()).await;
    });
}
