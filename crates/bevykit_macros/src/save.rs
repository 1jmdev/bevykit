//! `#[derive(SaveData)]`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, LitInt, LitStr};

use crate::action::to_snake_case;
use crate::paths::member_crate;

pub fn derive(input: DeriveInput) -> syn::Result<TokenStream> {
    let mut version: u32 = 1;
    let mut name = to_snake_case(&input.ident.to_string());
    for attribute in &input.attrs {
        if !attribute.path().is_ident("save") {
            continue;
        }
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("version") {
                version = meta.value()?.parse::<LitInt>()?.base10_parse()?;
                Ok(())
            } else if meta.path.is_ident("name") {
                name = meta.value()?.parse::<LitStr>()?.value();
                Ok(())
            } else {
                Err(meta.error("expected `version = N` or `name = \"...\"`"))
            }
        })?;
    }

    let data_crate = member_crate("bevykit_data");
    let ident = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #data_crate::save::SaveData for #ident #type_generics #where_clause {
            const VERSION: u32 = #version;
            const NAME: &'static str = #name;
        }
    })
}
