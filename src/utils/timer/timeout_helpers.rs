use std::{
    future::Future,
    time::{Duration, Instant},
};

use futures::{future::Either, future::select, pin_mut};
use parse_display::Display;

use super::{sleep, sleep_until};

#[derive(Debug, Display, PartialEq, Eq)]
#[display("should timeout")]
pub struct ShouldTimeoutError {
    _private: (),
}
impl ShouldTimeoutError {
    fn new() -> Self {
        Self { _private: () }
    }
}

impl std::error::Error for ShouldTimeoutError {}

#[doc(hidden)]
pub async fn with_should_timeout_async(
    fut: impl Future<Output = ()>,
    duration: Duration,
) -> Result<(), ShouldTimeoutError> {
    let timeout = sleep(duration);
    pin_mut!(fut);
    pin_mut!(timeout);
    match select(fut, timeout).await {
        Either::Left(((), _)) => Err(ShouldTimeoutError::new()),
        Either::Right(((), _fut)) => Ok(()),
    }
}

#[doc(hidden)]
pub async fn with_should_timeout_until_async(
    fut: impl Future<Output = ()>,
    instant: Instant,
) -> Result<(), ShouldTimeoutError> {
    let timeout = sleep_until(instant);
    pin_mut!(fut);
    pin_mut!(timeout);
    match select(fut, timeout).await {
        Either::Left(((), _)) => Err(ShouldTimeoutError::new()),
        Either::Right(((), _fut)) => Ok(()),
    }
}

#[doc(hidden)]
pub fn with_should_timeout(
    f: impl FnOnce() + Send + 'static,
    duration: Duration,
) -> Result<(), ShouldTimeoutError> {
    with_should_timeout_until(f, Instant::now() + duration)
}

#[doc(hidden)]
pub fn with_should_timeout_until(
    f: impl FnOnce() + Send + 'static,
    instant: Instant,
) -> Result<(), ShouldTimeoutError> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        f();
        let _ = tx.send(());
    });
    let timeout = instant.saturating_duration_since(Instant::now());
    match rx.recv_timeout(timeout) {
        Ok(()) => Err(ShouldTimeoutError::new()),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(()),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(ShouldTimeoutError::new()),
    }
}

pub trait IntoTimeoutDuration {
    fn into_timeout_duration(self) -> Duration;
}
impl IntoTimeoutDuration for Duration {
    fn into_timeout_duration(self) -> Duration {
        self
    }
}
impl IntoTimeoutDuration for &str {
    fn into_timeout_duration(self) -> Duration {
        parse_timeout_duration_str(self).unwrap_or_else(|err| panic!("{err}"))
    }
}
impl IntoTimeoutDuration for String {
    fn into_timeout_duration(self) -> Duration {
        parse_timeout_duration_str(self.as_str()).unwrap_or_else(|err| panic!("{err}"))
    }
}
impl IntoTimeoutDuration for &String {
    fn into_timeout_duration(self) -> Duration {
        parse_timeout_duration_str(self.as_str()).unwrap_or_else(|err| panic!("{err}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParseTimeoutDurationError(&'static str);

impl std::fmt::Display for ParseTimeoutDurationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

fn parse_timeout_duration_str(raw: &str) -> Result<Duration, ParseTimeoutDurationError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(ParseTimeoutDurationError("duration literal is empty"));
    }

    let (number, unit) = if let Some(prefix) = s.strip_suffix("ms") {
        (prefix, "ms")
    } else if let Some(prefix) = s.strip_suffix('s') {
        (prefix, "s")
    } else if let Some(prefix) = s.strip_suffix('m') {
        (prefix, "m")
    } else {
        return Err(ParseTimeoutDurationError("invalid duration literal"));
    };

    if number.is_empty() {
        return Err(ParseTimeoutDurationError("invalid duration literal"));
    }
    let value: f64 = number
        .parse()
        .map_err(|_| ParseTimeoutDurationError("invalid duration number"))?;
    if !value.is_finite() || value < 0.0 {
        return Err(ParseTimeoutDurationError(
            "duration must be non-negative and finite",
        ));
    }

    let secs = match unit {
        "ms" => value / 1000.0,
        "s" => value,
        "m" => value * 60.0,
        _ => unreachable!(),
    };
    Ok(Duration::from_secs_f64(secs))
}
