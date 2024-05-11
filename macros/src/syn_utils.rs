use proc_macro2::TokenStream;
use syn::Result;

macro_rules! bail {
    (_, $($arg:tt)*) => {
        bail!(::proc_macro2::Span::call_site(), $($arg)*)
    };
    ($span:expr, $fmt:literal $(,)?) => {
        return ::std::result::Result::Err(::syn::Error::new($span, ::std::format!($fmt)))
    };
    ($span:expr, $fmt:literal, $($arg:tt)*) => {
        return ::std::result::Result::Err(::syn::Error::new($span, ::std::format!($fmt, $($arg)*)))
    };
}

pub fn into_macro_output(input: Result<TokenStream>) -> proc_macro::TokenStream {
    match input {
        Ok(s) => s,
        Err(e) => e.to_compile_error(),
    }
    .into()
}
