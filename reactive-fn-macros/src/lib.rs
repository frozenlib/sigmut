use syn_utils::into_macro_output;

#[macro_use]
mod syn_utils;

mod bounds;
mod derive_observable_fmt;
mod obs_format_macro;

#[proc_macro_derive(ObservableFmt, attributes(observable_fmt))]
pub fn derive_observable_fmt(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    into_macro_output(derive_observable_fmt::derive_observable_fmt(input.into()))
}

#[proc_macro]
pub fn obs_format_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    into_macro_output(obs_format_macro::obs_format(input.into(), false))
}
#[proc_macro]
pub fn obs_format_impl_dump(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    into_macro_output(obs_format_macro::obs_format(input.into(), true))
}

#[proc_macro]
pub fn watch_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    into_macro_output(obs_format_macro::watch(input.into(), false))
}

#[proc_macro]
pub fn watch_impl_dump(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    into_macro_output(obs_format_macro::watch(input.into(), true))
}
