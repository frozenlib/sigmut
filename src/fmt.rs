#[doc(hidden)]
pub mod helpers;

#[cfg(test)]
mod tests;

/// Creates [`Signal<str>`](crate::Signal<str>) using interpolation of runtime expressions.
///
/// You can use the same syntax for arguments as with [`std::format!`].
/// However, in places where an expression of type `T` can be specified with [`std::format!`],
/// you can specify an expression of either `T` or [`ToSignal<T>`](crate::signal::ToSignal<T>).
///
/// # Examples
///
/// ```rust
/// use sigmut::{signal_format, State};
///
/// let mut rt = sigmut::core::Runtime::new();
///
/// let a = State::new(1);
/// let b = 2;
/// let s = signal_format!("{a}, {b}");
///
/// assert_eq!(s.get(&mut rt.sc()), "1, 2");
///
/// a.set(3, rt.ac());
/// assert_eq!(s.get(&mut rt.sc()), "3, 2");
/// ```
#[macro_export]
macro_rules! signal_format {
    ($($input:tt)*) => {
        $crate::fmt::helpers::signal_format_raw!($crate, $($input)*)
    };

}

#[doc(hidden)]
#[macro_export]
macro_rules! signal_format_dump {
    ($($input:tt)*) => {
        $crate::fmt::helpers::signal_format_dump_raw!($crate, $($input)*)
    };
}
