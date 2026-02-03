use std::time::Duration;

use futures::executor::block_on;
use pretty_assertions::assert_eq;
use sigmut::utils::timer::timeout_helpers::ShouldTimeoutError;
use sigmut::utils::timer::{should_timeout, sleep};

#[test]
fn should_timeout_returns_ok_after_timeout_for_result() {
    #[should_timeout("10ms")]
    async fn slow_result() -> Result<(), ShouldTimeoutError> {
        sleep(Duration::from_millis(30)).await;
        Ok::<(), ShouldTimeoutError>(())
    }

    block_on(slow_result()).unwrap();
    assert_eq!((), ());
}

#[test]
fn should_timeout_returns_error_when_no_timeout_for_result() {
    #[derive(Debug, PartialEq)]
    enum TestErr {
        ShouldTimeout,
    }

    impl From<ShouldTimeoutError> for TestErr {
        fn from(_: ShouldTimeoutError) -> Self {
            Self::ShouldTimeout
        }
    }

    #[should_timeout("50ms")]
    async fn fast_result() -> Result<(), TestErr> {
        sleep(Duration::from_millis(5)).await;
        Ok::<(), TestErr>(())
    }

    let err = block_on(fast_result()).unwrap_err();
    assert_eq!(err, TestErr::ShouldTimeout);
}

#[test]
#[should_panic]
fn should_timeout_panics_for_non_result_without_timeout() {
    #[should_timeout("50ms")]
    async fn fast_non_result() {
        sleep(Duration::from_millis(5)).await;
    }

    block_on(fast_non_result());
}

#[test]
fn should_timeout_allows_non_result_after_timeout() {
    #[should_timeout("10ms")]
    async fn slow_non_result() {
        sleep(Duration::from_millis(30)).await;
    }

    block_on(slow_non_result());
}

#[test]
fn sync_should_timeout_returns_ok_after_timeout_for_result() {
    #[should_timeout("10ms")]
    fn slow_result() -> Result<(), ShouldTimeoutError> {
        std::thread::sleep(Duration::from_millis(30));
        Ok::<(), ShouldTimeoutError>(())
    }

    slow_result().unwrap();
    assert_eq!((), ());
}

#[test]
#[should_panic]
fn sync_should_timeout_panics_for_non_result_without_timeout() {
    #[should_timeout("50ms")]
    fn fast_non_result() {
        std::thread::sleep(Duration::from_millis(5));
    }

    fast_non_result();
}
