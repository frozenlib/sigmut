use futures::stream::StreamExt;
use reactive_fn::*;
use reactive_fn_smol::*;
use std::{
    fmt::Debug,
    future::Future,
    sync::mpsc::{channel, Receiver, RecvTimeoutError},
    task::Poll,
    time::Duration,
};

fn local(fut: impl Future<Output = ()> + 'static) -> impl Future<Output = ()> {
    LOCAL_EXECUTOR.with(|sp| sp.spawn(fut))
}
fn spawn(fut: impl Future<Output = ()> + 'static + Send) -> impl Future<Output = ()> {
    smol::spawn(fut)
}
fn sleep(dur: Duration) -> impl Future {
    smol::Timer::after(dur)
}
async fn timeout<T>(dur: Duration, fut: impl Future<Output = T> + Unpin) -> Option<T> {
    use futures::future::{select, Either};
    match select(fut, sleep(dur)).await {
        Either::Left(x) => Some(x.0),
        Either::Right(_) => None,
    }
}
fn run(f: impl Future<Output = ()>) {
    LOCAL_EXECUTOR.with(|ex| smol::block_on(ex.run(f)))
}

const DUR: Duration = Duration::from_millis(300);

fn send_values<T: 'static + Copy>(cell: &ObsCell<T>, values: Vec<T>, dur: Duration) -> impl Future {
    let cell = cell.clone();
    local(async move {
        for value in values {
            sleep(dur).await;
            cell.set(value);
        }
    })
}

fn send_values_ref<T: 'static>(cell: &ObsRefCell<T>, values: Vec<T>, dur: Duration) -> impl Future {
    let cell = cell.clone();
    local(async move {
        for value in values {
            sleep(dur).await;
            cell.set(value);
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
            sleep(dur).await;
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
async fn assert_values<T>(source: DynObs<T>, values: Vec<T>, dur: Duration)
where
    T: 'static + PartialEq + Debug,
{
    let mut s = source.stream();
    for value in values {
        assert_eq!(timeout(dur, s.next()).await, Some(Some(value)));
    }
    assert_eq!(timeout(dur, s.next()).await, None);
}

#[test]
fn re_to_stream() {
    run(async {
        let cell = ObsCell::new(1);
        let _task = send_values(&cell, vec![5, 6], DUR);
        assert_values(cell.as_dyn(), vec![1, 5, 6], DUR * 2).await;
    });
}

#[test]
fn re_map_async() {
    run(async {
        let cell = ObsCell::new(1);
        let r = cell
            .as_dyn()
            .map_async(|x| async move {
                sleep(DUR / 2).await;
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
        let cell = ObsCell::new(1);
        let r = cell
            .as_dyn()
            .map_async(|x| async move {
                sleep(DUR).await;
                x + 2
            })
            .cloned();
        let _task = send_values(&cell, vec![10, 20, 30, 40], DUR / 2);
        let values = vec![Poll::Pending, Poll::Ready(42)];
        assert_values(r, values, DUR * 5).await;
    });
}

#[test]
fn re_subscribe() {
    run(async {
        let cell = ObsCell::new(1);
        let (s, r) = channel();

        let _s = cell.as_dyn().subscribe_async(move |x| {
            let s = s.clone();
            spawn(async move {
                s.send(x).unwrap();
            })
        });
        let _task = send_values(&cell, vec![10, 20, 30], DUR);
        assert_recv(r, vec![1, 10, 20, 30], DUR * 2).await;
    });
}

#[test]
fn obs_ref_map_async() {
    run(async {
        let cell = ObsRefCell::new(1);
        let r = cell
            .as_dyn_ref()
            .map_async(|&x| async move {
                sleep(DUR / 2).await;
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
fn obs_ref_subscribe() {
    run(async {
        let cell = ObsRefCell::new(1);
        let (s, r) = channel();

        let _s = cell.as_dyn_ref().subscribe_async(move |&x| {
            let s = s.clone();
            spawn(async move {
                s.send(x).unwrap();
            })
        });
        let _task = send_values_ref(&cell, vec![10, 20, 30], DUR);
        assert_recv(r, vec![1, 10, 20, 30], DUR * 2).await;
    });
}
