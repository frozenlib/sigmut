use proc_macro::TokenStream;
use syn_utils::into_macro_output;

#[macro_use]
mod syn_utils;

mod signal_format_impl;
mod timeout_impl;

#[proc_macro]
pub fn signal_format_raw(input: proc_macro::TokenStream) -> TokenStream {
    into_macro_output(signal_format_impl::signal_format(input.into()))
}

#[proc_macro]
pub fn signal_format_dump_raw(input: proc_macro::TokenStream) -> TokenStream {
    into_macro_output(signal_format_impl::signal_format_dump(input.into()))
}

/// Adds a timeout to a function.
///
/// This attribute can be applied to both synchronous and asynchronous functions.
///
/// # Arguments
///
/// The timeout duration can be specified in the same way as [`should_timeout`].
///
/// Any type that implements
/// [`IntoTimeoutDuration`](sigmut::utils::timer::timeout_helpers::IntoTimeoutDuration) is accepted. This includes:
/// - A string literal with a unit suffix (see table below)
/// - A `Duration` expression: `std::time::Duration::from_millis(100)`
///
/// | Suffix | Equivalent to                      |
/// |--------|----------------------------------- |
/// | `ms`   | `Duration::from_millis(n)`         |
/// | `s`    | `Duration::from_secs(n)`           |
/// | `m`    | `Duration::from_secs(n * 60)`      |
///
/// # Behavior
///
/// - If the function returns `Result<T, E>`, a timeout returns `Err` with
///   [`timer::TimeoutError`](sigmut::utils::timer::TimeoutError) converted via `Into::into`.
/// - If the function does not return `Result`, a timeout causes a panic.
///
/// # Examples
///
/// ```ignore
/// #[timeout("100ms")]
/// async fn fetch_data() -> Result<Data, MyError> {
///     // ...
/// }
///
/// #[timeout(std::time::Duration::from_secs(5))]
/// fn blocking_operation() -> Result<(), timer::TimeoutError> {
///     // ...
/// }
/// ```
#[proc_macro_attribute]
pub fn timeout(attr: proc_macro::TokenStream, item: proc_macro::TokenStream) -> TokenStream {
    into_macro_output(timeout_impl::timeout(attr.into(), item.into()))
}

/// Ensures a function times out; if it doesn't, returns an error or panics.
///
/// This attribute can be applied to both synchronous and asynchronous functions.
///
/// # Arguments
///
/// The timeout duration can be specified in the same way as [`timeout`].
///
/// Any type that implements
/// [`IntoTimeoutDuration`](sigmut::utils::timer::timeout_helpers::IntoTimeoutDuration) is accepted. This includes:
/// - A string literal with a unit suffix (see table below)
/// - A `Duration` expression: `std::time::Duration::from_millis(100)`
///
/// | Suffix | Equivalent to                      |
/// |--------|----------------------------------- |
/// | `ms`   | `Duration::from_millis(n)`         |
/// | `s`    | `Duration::from_secs(n)`           |
/// | `m`    | `Duration::from_secs(n * 60)`      |
///
/// # Behavior
///
/// - Only functions returning `()` or `Result<(), E>` are supported.
/// - If the function returns `Result<(), E>`, completing before the timeout returns `Err` with
///   [`timer::timeout_helpers::ShouldTimeoutError`](sigmut::utils::timer::timeout_helpers::ShouldTimeoutError)
///   converted via `Into::into`.
/// - If the function returns `()`, completing before the timeout causes a panic.
/// - If the timeout elapses, control returns immediately with `()` or `Ok(())`.
///
/// # Examples
///
/// ```ignore
/// #[should_timeout("100ms")]
/// async fn slow_fetch() -> Result<(), MyError> {
///     // ...
///     Ok(())
/// }
///
/// #[should_timeout(std::time::Duration::from_secs(5))]
/// fn slow_operation() -> Result<(), timer::timeout_helpers::ShouldTimeoutError> {
///     // ...
///     Ok(())
/// }
/// ```
#[proc_macro_attribute]
pub fn should_timeout(attr: proc_macro::TokenStream, item: proc_macro::TokenStream) -> TokenStream {
    into_macro_output(timeout_impl::should_timeout(attr.into(), item.into()))
}
