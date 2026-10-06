//! `#[derive(KitAction)]`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields, LitStr};

use crate::paths::member_crate;

pub fn derive(input: DeriveInput) -> syn::Result<TokenStream> {
    let Data::Enum(data) = &input.data else {
        return Err(Error::new_spanned(
            &input.ident,
            "KitAction can only be derived for enums",
        ));
    };

    let mut variants = Vec::new();
    let mut names = Vec::new();
    for variant in &data.variants {
        if !matches!(variant.fields, Fields::Unit) {
            return Err(Error::new_spanned(
                variant,
                "KitAction variants must not carry data",
            ));
        }
        let mut name = to_snake_case(&variant.ident.to_string());
        for attribute in &variant.attrs {
            if !attribute.path().is_ident("action") {
                continue;
            }
            attribute.parse_nested_meta(|meta| {
                if meta.path.is_ident("name") {
                    name = meta.value()?.parse::<LitStr>()?.value();
                    Ok(())
                } else {
                    Err(meta.error("expected `name = \"...\"`"))
                }
            })?;
        }
        variants.push(&variant.ident);
        names.push(name);
    }

    let input_crate = member_crate("bevykit_input");
    let ident = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();

    Ok(quote! {
        impl #impl_generics #input_crate::action::Action for #ident #type_generics #where_clause {
            fn variants() -> &'static [Self] {
                &[#(Self::#variants),*]
            }

            fn name(&self) -> &'static str {
                match self {
                    #(Self::#variants => #names,)*
                }
            }
        }
    })
}

/// Converts `CamelCase` to `snake_case`.
pub fn to_snake_case(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 4);
    let mut previous_lowercase = false;
    for character in value.chars() {
        if character.is_uppercase() {
            if previous_lowercase {
                output.push('_');
            }
            output.extend(character.to_lowercase());
            previous_lowercase = false;
        } else {
            previous_lowercase = character.is_lowercase() || character.is_ascii_digit();
            output.push(character);
        }
    }
    output
}
