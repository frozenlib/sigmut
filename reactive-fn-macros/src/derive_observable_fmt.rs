use crate::{
    bounds::{Bound, Bounds, WhereClauseBuilder},
    syn_utils::{to_root_path, try_dump},
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use structmeta::{NameArgs, StructMeta};
use syn::{parse::Parse, parse2, spanned::Spanned, Attribute, DeriveInput, Ident, Result};

pub fn derive_observable_fmt(input: TokenStream) -> Result<TokenStream> {
    let input: DeriveInput = parse2(input)?;
    let args: AttrArgs = parse_attrs("observable_fmt", &input.attrs)?;
    let ns = to_root_path(args.self_crate);
    let mut wcb = WhereClauseBuilder::new(&input.generics);
    wcb.push_bounds(&Bounds::from(&args.bound));

    let mut ts = TokenStream::new();
    let (impl_g, type_g, _) = input.generics.split_for_impl();

    for t in FmtTrait::ITEMS {
        let id_self = &input.ident;
        let id_obs_fmt = t.ident_obs_fmt();
        let wc = wcb.build(|ty| quote!(#ty : #ns::fmt::#id_obs_fmt));
        ts.extend(quote! {
            impl #impl_g #ns::fmt::#id_obs_fmt for #id_self #type_g #wc {
                fn obs_fmt(&self, f: &mut ::std::fmt::Formatter, oc: &mut #ns::core::ObsContext) -> ::std::fmt::Result {
                    #ns::observable::Observable::with(self, |value, oc| value.obs_fmt(f, oc), oc)
                }
            }
        });
    }
    try_dump(ts, args.dump)
}

fn parse_attrs<T: Parse + Default>(name: &str, attrs: &[Attribute]) -> Result<T> {
    let mut result = None::<T>;
    for attr in attrs {
        if attr.path.is_ident(name) {
            if result.is_some() {
                bail!(attr.span(), "`#[{name}]` can be specified only once")
            }
            result = Some(attr.parse_args()?);
        }
    }
    Ok(result.unwrap_or_default())
}

#[derive(StructMeta, Default)]
struct AttrArgs {
    dump: bool,
    self_crate: bool,
    bound: Option<NameArgs<Vec<Bound>>>,
}

struct FmtTrait(&'static str);
impl FmtTrait {
    const ITEMS: &[Self] = &[
        Self::new("Binary"),
        Self::new("Display"),
        Self::new("Debug"),
        Self::new("LowerExp"),
        Self::new("LowerHex"),
        Self::new("Octal"),
        Self::new("Pointer"),
        Self::new("UpperExp"),
        Self::new("UpperHex"),
    ];

    const fn new(id: &'static str) -> Self {
        Self(id)
    }
    fn ident_obs_fmt(&self) -> Ident {
        format_ident!("Observable{}", self.0)
    }
}
