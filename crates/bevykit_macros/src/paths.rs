//! Resolution of the path through which generated code refers to bevykit.

use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::Ident;

/// Returns the path of a bevykit member crate as seen from the crate being compiled.
///
/// Games usually depend on the `bevykit` facade, in which case `bevykit_input` is reached as
/// `::bevykit::input`. Crates depending on a member crate directly use its own name.
pub fn member_crate(member: &str) -> TokenStream {
    let module = member.trim_start_matches("bevykit_");
    if let Ok(found) = crate_name("bevykit") {
        let root = found_to_tokens(found, "bevykit");
        let module = Ident::new(module, Span::call_site());
        return quote!(#root::#module);
    }
    match crate_name(member) {
        Ok(found) => found_to_tokens(found, member),
        Err(_) => {
            let ident = Ident::new(member, Span::call_site());
            quote!(::#ident)
        }
    }
}

fn found_to_tokens(found: FoundCrate, original: &str) -> TokenStream {
    match found {
        FoundCrate::Itself => {
            let ident = Ident::new(original, Span::call_site());
            quote!(::#ident)
        }
        FoundCrate::Name(name) => {
            let ident = Ident::new(&name, Span::call_site());
            quote!(::#ident)
        }
    }
}
