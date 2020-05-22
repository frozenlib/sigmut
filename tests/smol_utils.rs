use futures::{
    future::{select, Either},
    stream::StreamExt,
    Future,
};
use reactive_fn::smol_utils::*;
use reactive_fn::*;
use smol::{Task, Timer};
use std::{fmt::Debug, task::Poll, time::Duration};
use thiserror::Error;

const NEXT_DURATION: Duration = Duration::from_millis(300);

fn send_values<T: 'static + Copy>(cell: &ReCell<T>, values: Vec<T>) -> Task<()> {
    let cell = cell.clone();
    Task::local(async move {
        for value in values {
            Timer::after(NEXT_DURATION).await;
            cell.set_and_update(value);
        }
    })
}
async fn assert_values<T>(source: Re<T>, values: Vec<T>)
where
    T: 'static + PartialEq + Debug,
{
    let mut s = source.to_stream();
    let dur = NEXT_DURATION * 2;
    for value in values {
        assert_eq!(timeout(s.next(), dur).await, Ok(Some(value)));
    }
    assert_eq!(timeout(s.next(), dur).await, Err(TimeoutError));
}

#[derive(Error, Debug, Eq, PartialEq)]
#[error("timeout")]
struct TimeoutError;

async fn timeout<T>(
    fut: impl Future<Output = T> + Unpin,
    dur: Duration,
) -> std::result::Result<T, TimeoutError> {
    match select(fut, Timer::after(dur)).await {
        Either::Left(x) => Ok(x.0),
        Either::Right(_) => Err(TimeoutError),
    }
}

#[test]
fn re_to_stream() {
    smol::run(async {
        let cell = ReCell::new(1);
        let _task = send_values(&cell, vec![5, 6]);
        assert_values(cell.to_re(), vec![1, 5, 6]).await;
    });
}

#[test]
fn re_map_async() {
    smol::run(async {
        let cell = ReCell::new(1);
        let r = cell
            .to_re()
            .map_async_with(
                |x| async move {
                    Timer::after(NEXT_DURATION / 2).await;
                    x + 2
                },
                LocalSpawner,
            )
            .cloned();
        let _task = send_values(&cell, vec![5, 10]);
        let values = vec![
            Poll::Pending,
            Poll::Ready(3),
            Poll::Pending,
            Poll::Ready(7),
            Poll::Pending,
            Poll::Ready(12),
        ];
        assert_values(r, values).await;
    });
}
