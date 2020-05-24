use futures::{
    future::{select, Either},
    stream::StreamExt,
    Future,
};
use reactive_fn::smol_utils::*;
use reactive_fn::*;
use smol::{Task, Timer};
use std::{
    fmt::Debug,
    sync::mpsc::{channel, Receiver, RecvTimeoutError},
    task::Poll,
    time::Duration,
};
use thiserror::Error;

const DUR: Duration = Duration::from_millis(300);

fn send_values<T: 'static + Copy>(cell: &ReCell<T>, values: Vec<T>, dur: Duration) -> Task<()> {
    let cell = cell.clone();
    Task::local(async move {
        for value in values {
            Timer::after(dur).await;
            cell.set_and_update(value);
        }
    })
}

fn send_values_ref<T: 'static>(cell: &ReRefCell<T>, values: Vec<T>, dur: Duration) -> Task<()> {
    let cell = cell.clone();
    Task::local(async move {
        for value in values {
            Timer::after(dur).await;
            cell.set_and_update(value);
        }
    })
}

async fn assert_recv<T>(r: Receiver<T>, values: Vec<T>, dur: Duration)
where
    T: 'static + PartialEq + Debug,
{
    for value in values {
        let a = if let Ok(a) = r.try_recv() {
            a
        } else {
            Timer::after(dur).await;
            if let Ok(a) = r.try_recv() {
                a
            } else {
                panic!("value {:?} : timeout.", value);
            }
        };
        assert_eq!(a, value);
    }
    assert_eq!(r.recv_timeout(dur), Err(RecvTimeoutError::Timeout));
}
async fn assert_values<T>(source: Re<T>, values: Vec<T>, dur: Duration)
where
    T: 'static + PartialEq + Debug,
{
    let mut s = source.to_stream();
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

fn run(f: impl Future<Output = ()>) {
    smol::run(f);
}

#[test]
fn re_to_stream() {
    run(async {
        let cell = ReCell::new(1);
        let _task = send_values(&cell, vec![5, 6], DUR);
        assert_values(cell.to_re(), vec![1, 5, 6], DUR * 2).await;
    });
}

#[test]
fn re_map_async() {
    run(async {
        let cell = ReCell::new(1);
        let r = cell
            .to_re()
            .map_async(|x| async move {
                Timer::after(DUR / 2).await;
                x + 2
            })
            .cloned();
        let _task = send_values(&cell, vec![5, 10], DUR);
        let values = vec![
            Poll::Pending,
            Poll::Ready(3),
            Poll::Pending,
            Poll::Ready(7),
            Poll::Pending,
            Poll::Ready(12),
        ];
        assert_values(r, values, DUR * 2).await;
    });
}

#[test]
fn re_map_async_cancel() {
    run(async {
        let cell = ReCell::new(1);
        let r = cell
            .to_re()
            .map_async(|x| async move {
                Timer::after(DUR).await;
                x + 2
            })
            .cloned();
        let _task = send_values(&cell, vec![10, 20, 30, 40], DUR / 2);
        let values = vec![Poll::Pending, Poll::Ready(42)];
        assert_values(r, values, DUR * 5).await;
    });
}

#[test]
fn re_for_each() {
    run(async {
        let cell = ReCell::new(1);
        let (s, r) = channel();

        let _s = cell.to_re().for_each_async(move |x| {
            let s = s.clone();
            Task::spawn(async move {
                s.send(x).unwrap();
            })
        });
        let _task = send_values(&cell, vec![10, 20, 30], DUR);
        assert_recv(r, vec![1, 10, 20, 30], DUR).await;
    });
}

#[test]
fn re_ref_map_async() {
    run(async {
        let cell = ReRefCell::new(1);
        let r = cell
            .to_re_ref()
            .map_async(|&x| async move {
                Timer::after(DUR / 2).await;
                x + 2
            })
            .cloned();
        let _task = send_values_ref(&cell, vec![5, 10], DUR);
        let values = vec![
            Poll::Pending,
            Poll::Ready(3),
            Poll::Pending,
            Poll::Ready(7),
            Poll::Pending,
            Poll::Ready(12),
        ];
        assert_values(r, values, DUR * 2).await;
    });
}

#[test]
fn re_ref_for_each() {
    run(async {
        let cell = ReRefCell::new(1);
        let (s, r) = channel();

        let _s = cell.to_re_ref().for_each_async(move |&x| {
            let s = s.clone();
            Task::spawn(async move {
                s.send(x).unwrap();
            })
        });
        let _task = send_values_ref(&cell, vec![10, 20, 30], DUR);
        assert_recv(r, vec![1, 10, 20, 30], DUR).await;
    });
}
