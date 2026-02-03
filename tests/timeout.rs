use std::time::Duration;

use futures::executor::block_on;
use pretty_assertions::assert_eq;
use sigmut::utils::timer::{TimeoutError, sleep, timeout};

#[test]
fn timeout_returns_ok_for_result() {
    #[timeout("50ms")]
    async fn ok_result() -> Result<u32, TimeoutError> {
        sleep(Duration::from_millis(5)).await;
        Ok(7)
    }

    let value = block_on(ok_result()).unwrap();
    assert_eq!(value, 7);
}

#[test]
fn timeout_converts_error_for_result() {
    #[derive(Debug, PartialEq)]
    enum TestErr {
        Timeout,
    }

    impl From<TimeoutError> for TestErr {
        fn from(_: TimeoutError) -> Self {
            Self::Timeout
        }
    }

    #[timeout("10ms")]
    async fn timeout_result() -> Result<u32, TestErr> {
        sleep(Duration::from_millis(50)).await;
        Ok(7)
    }

    let err = block_on(timeout_result()).unwrap_err();
    assert_eq!(err, TestErr::Timeout);
}

#[test]
fn timeout_returns_timeout_error_for_result() {
    #[timeout("10ms")]
    async fn timeout_result() -> Result<u32, TimeoutError> {
        sleep(Duration::from_millis(50)).await;
        Ok(7)
    }

    let result = block_on(timeout_result());
    assert!(result.is_err());
}

#[test]
fn timeout_accepts_duration_expr() {
    #[timeout(std::time::Duration::from_millis(50))]
    async fn ok_with_duration_expr() -> Result<u32, TimeoutError> {
        Ok(9)
    }

    let value = block_on(ok_with_duration_expr()).unwrap();
    assert_eq!(value, 9);
}

#[test]
fn timeout_accepts_minutes_literal() {
    #[timeout("1.5m")]
    async fn ok_with_minutes_literal() -> Result<u32, TimeoutError> {
        Ok(3)
    }

    let value = block_on(ok_with_minutes_literal()).unwrap();
    assert_eq!(value, 3);
}

fn timeout_string_duration() -> String {
    "25ms".to_string()
}

#[test]
fn timeout_accepts_string_expr() {
    #[timeout(timeout_string_duration())]
    async fn ok_with_string_expr() -> Result<u32, TimeoutError> {
        Ok(5)
    }

    let value = block_on(ok_with_string_expr()).unwrap();
    assert_eq!(value, 5);
}

#[test]
#[should_panic]
fn timeout_panics_for_non_result() {
    #[timeout("10ms")]
    async fn timeout_panics() -> u32 {
        sleep(Duration::from_millis(50)).await;
        1
    }
    block_on(timeout_panics());
}

#[test]
fn sync_timeout_returns_ok_for_result() {
    #[timeout("100ms")]
    fn ok_result() -> Result<u32, TimeoutError> {
        std::thread::sleep(Duration::from_millis(5));
        Ok(7)
    }

    let value = ok_result().unwrap();
    assert_eq!(value, 7);
}

#[test]
fn sync_timeout_returns_timeout_error_for_result() {
    #[timeout("10ms")]
    fn timeout_result() -> Result<u32, TimeoutError> {
        std::thread::sleep(Duration::from_millis(100));
        Ok(7)
    }

    let result = timeout_result();
    assert!(result.is_err());
}

#[test]
#[should_panic]
fn sync_timeout_panics_for_non_result() {
    #[timeout("10ms")]
    fn timeout_panics() -> u32 {
        std::thread::sleep(Duration::from_millis(100));
        1
    }
    timeout_panics();
}
