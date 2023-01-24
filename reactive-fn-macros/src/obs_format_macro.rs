use once_cell::sync::Lazy;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use regex::Regex;
use std::collections::BTreeSet;
use structmeta::ToTokens;
use syn::{
    braced,
    parse::{Parse, ParseStream},
    parse2, parse_quote, Expr, Ident, LitStr, Path, Result, Token,
};

use crate::syn_utils::try_dump;

pub fn obs_format(input: TokenStream, dump: bool) -> Result<TokenStream> {
    let args: FormatArgs = parse2(input)?;
    let ns = &args.ns;
    let mut lets = Vec::new();
    let mut format_args = Vec::new();
    let mut index = 0;
    for arg in make_args(&args.format_str, args.args) {
        let value = &arg.value;
        if let Some(key) = &arg.key {
            lets.push(quote!(#key = #value));
            format_args.push(quote!(#key = #key));
        } else {
            let key = format_ident!("_{index}");
            lets.push(quote!(#key = #value));
            format_args.push(quote!(#key));
            index += 1;
        }
    }
    let format_str = &args.format_str;
    let ts = quote! { {
        #(let #lets;)*
        #ns::fmt::ObsDisplay::new(move |f, oc| {
            #ns::watch_write!(f, oc, #format_str #(, #format_args)*)
        }).into_obs()
    }};
    try_dump(ts, dump)
}

pub fn watch(input: TokenStream, dump: bool) -> Result<TokenStream> {
    let args: WatchArgs = parse2(input)?;
    let ns = &args.ns;
    let mut format_args = Vec::new();
    let oc = Ident::new("oc", Span::mixed_site());
    for arg in make_args(&args.format_str, args.args) {
        let value = &arg.value;
        let value = quote!(#ns::fmt::ObsFormatArg::new(&(#value), &#oc));
        if let Some(key) = &arg.key {
            format_args.push(quote!(#key = #value));
        } else {
            format_args.push(quote!(#value));
        }
    }
    let oc_orig = &args.oc;
    let path = &args.path;
    let preargs = &args.preargs;
    let format_str = &args.format_str;
    let ts = quote! {
        #ns::fmt::call_fmt(#oc_orig, |#oc| {
            #path!(#preargs #format_str #(, #format_args)*)
        })
    };
    try_dump(ts, dump)
}

struct FormatArgs {
    ns: Path,
    format_str: LitStr,
    args: Vec<Arg>,
}

impl Parse for FormatArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let ns = input.parse()?;
        let _ = input.parse::<Token![,]>()?;
        let format_str = input.parse()?;
        let args = Arg::parse_list(input)?;
        Ok(Self {
            ns,
            format_str,
            args,
        })
    }
}

struct WatchArgs {
    ns: Path,
    path: Path,
    preargs: TokenStream,
    oc: Expr,
    format_str: LitStr,
    args: Vec<Arg>,
}

impl Parse for WatchArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let ns = input.parse()?;
        let _ = input.parse::<Token![,]>()?;
        let path = input.parse()?;
        let _ = input.parse::<Token![,]>()?;
        let preargs;
        let _ = braced!(preargs in input);
        let preargs = preargs.parse()?;
        let _ = input.parse::<Token![,]>()?;
        let oc = input.parse()?;
        let _ = input.parse::<Token![,]>()?;
        let format_str = input.parse()?;
        let args = Arg::parse_list(input)?;

        Ok(Self {
            ns,
            oc,
            path,
            preargs,
            format_str,
            args,
        })
    }
}

#[derive(ToTokens)]
struct Arg {
    key: Option<Ident>,
    eq: Option<Token![=]>,
    value: Expr,
}
impl Arg {
    fn parse_list(input: ParseStream) -> Result<Vec<Self>> {
        let mut args = Vec::new();
        while !input.is_empty() {
            let _ = input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            args.push(input.parse()?);
        }
        Ok(args)
    }
}
impl Parse for Arg {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut key = None;
        let mut eq = None;
        if input.peek(Ident) && input.peek2(Token![=]) {
            key = Some(input.parse()?);
            eq = Some(input.parse()?);
        }
        let value = input.parse()?;
        Ok(Self { key, eq, value })
    }
}

fn keys_from_format(s: &str) -> BTreeSet<&str> {
    static RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"\{\{|\{(?P<key>[^:}]*)(:[^}]*)?\}").unwrap());
    let mut keys = BTreeSet::new();
    for i in RE.captures_iter(s) {
        if let Some(m) = i.name("key") {
            let key = m.as_str();
            if !key.is_empty() && key.parse::<usize>().is_err() {
                keys.insert(key);
            }
        }
    }
    keys
}
fn make_args(format: &LitStr, mut args: Vec<Arg>) -> Vec<Arg> {
    let format_str = format.value();
    let mut keys = keys_from_format(&format_str);
    for arg in args.iter() {
        if let Some(key) = &arg.key {
            keys.remove(key.to_string().as_str());
        }
    }
    for key in keys {
        let key = Ident::new(key, format.span());
        let value = parse_quote!(#key);
        args.push(Arg {
            key: Some(key),
            eq: parse_quote!(=),
            value,
        });
    }
    args
}
