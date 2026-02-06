use proc_macro::TokenStream;
use syn_utils::into_macro_output;

#[macro_use]
mod syn_utils;

mod signal_format_impl;

#[proc_macro]
pub fn signal_format_raw(input: proc_macro::TokenStream) -> TokenStream {
    into_macro_output(signal_format_impl::signal_format(input.into()))
}

#[proc_macro]
pub fn signal_format_dump_raw(input: proc_macro::TokenStream) -> TokenStream {
    into_macro_output(signal_format_impl::signal_format_dump(input.into()))
}
