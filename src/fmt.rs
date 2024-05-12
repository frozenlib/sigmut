#[doc(hidden)]
pub mod helpers;

#[cfg(test)]
mod tests;

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
