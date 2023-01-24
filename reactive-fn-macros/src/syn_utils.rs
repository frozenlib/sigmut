use proc_macro2::TokenStream;
use quote::quote;
use syn::{spanned::Spanned, Result};

macro_rules! bail {
    ($span:expr, $message:literal $(,)?) => {
        return std::result::Result::Err(syn::Error::new($span, $message))
    };
    ($span:expr, $err:expr $(,)?) => {
        return std::result::Result::Err(syn::Error::new($span, $err))
    };
    ($span:expr, $fmt:expr, $($arg:tt)*) => {
        return std::result::Result::Err(syn::Error::new($span, std::format!($fmt, $($arg)*)))
    };
}

pub fn into_macro_output(input: Result<TokenStream>) -> proc_macro::TokenStream {
    match input {
        Ok(s) => s,
        Err(e) => e.to_compile_error(),
    }
    .into()
}

// pub struct GenericParamSet {
//     idents: HashSet<Ident>,
// }

// impl GenericParamSet {
//     pub fn new(generics: &Generics) -> Self {
//         let mut idents = HashSet::new();
//         for p in &generics.params {
//             match p {
//                 GenericParam::Type(t) => {
//                     idents.insert(t.ident.unraw());
//                 }
//                 GenericParam::Const(t) => {
//                     idents.insert(t.ident.unraw());
//                 }
//                 _ => {}
//             }
//         }
//         Self { idents }
//     }
//     fn contains(&self, ident: &Ident) -> bool {
//         self.idents.contains(&ident.unraw())
//     }

//     pub fn contains_in_type(&self, ty: &Type) -> bool {
//         struct Visitor<'a> {
//             generics: &'a GenericParamSet,
//             result: bool,
//         }
//         impl<'a, 'ast> Visit<'ast> for Visitor<'a> {
//             fn visit_path(&mut self, i: &'ast syn::Path) {
//                 if i.leading_colon.is_none() {
//                     if let Some(s) = i.segments.iter().next() {
//                         if self.generics.contains(&s.ident) {
//                             self.result = true;
//                         }
//                     }
//                 }
//                 visit_path(self, i);
//             }
//         }
//         let mut visitor = Visitor {
//             generics: self,
//             result: false,
//         };
//         visitor.visit_type(ty);
//         visitor.result
//     }
// }

pub fn try_dump(ts: TokenStream, dump: bool) -> Result<TokenStream> {
    if dump {
        bail!(ts.span(), "macro result :\n {}", ts);
    }
    Ok(ts)
}
pub fn to_root_path(self_crate: bool) -> TokenStream {
    if self_crate {
        quote!(crate)
    } else {
        quote!(::reactive_fn)
    }
}
