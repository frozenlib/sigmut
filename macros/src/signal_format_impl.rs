use std::mem::take;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse2, parse_quote,
    punctuated::Punctuated,
    Expr, Ident, LitStr, Path, Result, Token,
};

pub fn signal_format_dump(input: TokenStream) -> Result<TokenStream> {
    let ts = signal_format(input)?;
    Ok(syn::Error::new(Span::call_site(), ts.to_string()).to_compile_error())
}

pub fn signal_format(input: TokenStream) -> Result<TokenStream> {
    let input: Input = parse2(input)?;
    let parts = parse_parts(&input.format)?;

    let c = &input.crate_path;
    let h = quote!(#c::fmt::helpers);
    let b = quote!(_signal_format_builder);

    let mut index = 0;
    let mut lets = Vec::new();
    let mut dummy_fn_args = Vec::new();
    for part in parts {
        let expr = match part {
            Part::Str(s) => {
                quote!(#b.push_static(#s))
            }
            Part::Var { key, format_spec } => {
                if let Some(dummy_fn_arg) = key.to_dummy_fn_arg(&input) {
                    dummy_fn_args.push(quote!(#dummy_fn_arg : #h::DummyArg));
                }
                let expr = key.to_expr(&mut index, &input)?;
                let s = format_str_from_spec(&format_spec);
                quote!(#h::Helper(&(#expr)).signal_fmt(#b, move |s, v| ::std::write!(s, #s, v)))
            }
        };
        lets.push(quote!(let #b = #expr;));
    }
    let mut dummy_args = Vec::new();
    for arg in &input.args {
        dummy_args.push(arg.to_dummy_arg(quote!(#h::DummyArg)));
    }

    let format_str = &input.format;
    Ok(quote! {
        {
            fn _dummy_for_rust_anazlyer(#(#dummy_fn_args,)*) {
                let _ = std::format!(#format_str #(,#dummy_args)*);
            }

            #[allow(unused_imports)]
            use ::std::fmt::Write;
            #[allow(unused_imports)]
            use #h::{SignalStringBuilder, HelperForNonSignal};
            let #b = #h::signal_string_builder();
            #(#lets)*
            #b.build()
        }
    })
}

macro_rules! regex {
    ($s:expr) => {{
        static RE: ::std::sync::OnceLock<regex::Regex> = ::std::sync::OnceLock::new();
        RE.get_or_init(|| ::regex::Regex::new($s).unwrap())
    }};
}

struct Input {
    crate_path: Path,
    _comma0: Token![,],
    format: LitStr,
    _comma1: Option<Token![,]>,
    args: Punctuated<Arg, Token![,]>,
}
impl Parse for Input {
    fn parse(input: ParseStream) -> Result<Self> {
        let crate_path = input.parse()?;
        let comma0 = input.parse()?;
        let format = input.parse()?;
        let comma1;
        let args;
        if input.is_empty() {
            comma1 = None;
            args = Punctuated::new();
        } else {
            comma1 = Some(input.parse()?);
            args = input.parse_terminated(Arg::parse, Token![,])?;
        }
        Ok(Self {
            crate_path,
            _comma0: comma0,
            format,
            _comma1: comma1,
            args,
        })
    }
}
impl Input {
    fn expr_by_name_opt(&self, name: &Ident) -> Option<Expr> {
        let target_name = name;
        for arg in &self.args {
            match arg {
                Arg::NameExpr { name, expr, .. } => {
                    if name == target_name {
                        return Some(expr.clone());
                    }
                }
                Arg::Expr { .. } => {}
            }
        }
        None
    }
    fn expr_by_name(&self, name: &Ident) -> Expr {
        self.expr_by_name_opt(name)
            .unwrap_or_else(|| parse_quote!(#name))
    }
    fn find_expr_by_index(&self, index: usize) -> Result<Expr> {
        if index < self.args.len() {
            Ok(self.args[index].expr().clone())
        } else {
            bail!(
                Span::call_site(),
                "invalid reference to positional argument {}",
                index
            );
        }
    }
}

enum Arg {
    NameExpr {
        name: Ident,
        _eq: Token![=],
        expr: Expr,
    },
    Expr {
        expr: Expr,
    },
}
impl Arg {
    fn expr(&self) -> &Expr {
        match self {
            Self::NameExpr { expr, .. } => expr,
            Self::Expr { expr } => expr,
        }
    }
    fn to_dummy_arg(&self, dummy_expr: TokenStream) -> TokenStream {
        match self {
            Self::NameExpr { name, .. } => quote!(#name = #dummy_expr),
            Self::Expr { .. } => dummy_expr,
        }
    }
}
impl Parse for Arg {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(Ident) && input.peek2(Token![=]) {
            let name = input.parse()?;
            let eq = input.parse()?;
            let expr = input.parse()?;
            Ok(Self::NameExpr {
                name,
                _eq: eq,
                expr,
            })
        } else {
            let expr = input.parse()?;
            Ok(Self::Expr { expr })
        }
    }
}

fn parse_parts(input: &LitStr) -> Result<Vec<Part>> {
    let regex_str = regex!(r"^[^{}]+");
    let regex_var = regex!(r"^\{([^:{}]*)(?::([^}]*))?\}");
    let s = input.value();
    let mut s = s.as_str();
    let mut parts = Vec::new();
    let span = input.span();
    let mut st = String::new();
    while !s.is_empty() {
        if s.starts_with("{{") {
            st.push('{');
            s = &s[2..];
            continue;
        }
        if s.starts_with("}}") {
            st.push('}');
            s = &s[2..];
            continue;
        }
        if let Some(m) = regex_str.find(s) {
            st.push_str(m.as_str());
            s = &s[m.end()..];
            continue;
        }
        if let Some(c) = regex_var.captures(s) {
            let key = VarKey::prase(c.get(1).unwrap().as_str(), span)?;
            let format_spec = c.get(2).map_or("", |x| x.as_str()).into();
            parts.push(Part::Str(take(&mut st)));
            parts.push(Part::Var { key, format_spec });
            s = &s[c.get(0).unwrap().end()..];
            continue;
        }
        bail!(span, "invalid format.");
    }
    parts.push(Part::Str(st));
    parts.retain(|x| !matches!(x, Part::Str(x) if x.is_empty()));
    Ok(parts)
}

enum Part {
    Str(String),
    Var { key: VarKey, format_spec: String },
}

enum VarKey {
    None,
    Name(Ident),
    Index(usize),
}

impl VarKey {
    fn prase(s: &str, span: Span) -> Result<Self> {
        if s.is_empty() {
            return Ok(Self::None);
        }
        if let Ok(index) = s.parse() {
            return Ok(Self::Index(index));
        }
        Ok(Self::Name(Ident::new(s, span)))
    }

    fn to_expr(&self, index: &mut usize, input: &Input) -> Result<Expr> {
        match self {
            Self::None => {
                let i = *index;
                *index += 1;
                input.find_expr_by_index(i)
            }
            Self::Name(name) => Ok(input.expr_by_name(name)),
            Self::Index(i) => input.find_expr_by_index(*i),
        }
    }
    fn to_dummy_fn_arg(&self, input: &Input) -> Option<&Ident> {
        match self {
            Self::Name(name) if input.expr_by_name_opt(name).is_none() => Some(name),
            _ => None,
        }
    }
}

fn format_str_from_spec(spen: &str) -> String {
    if spen.is_empty() {
        "{}".into()
    } else {
        format!("{{:{spen}}}")
    }
}
