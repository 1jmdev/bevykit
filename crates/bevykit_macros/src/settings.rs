//! `#[derive(Settings)]`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Error, Expr, Fields, LitStr, Path};

use crate::paths::member_crate;

pub fn derive(input: DeriveInput) -> syn::Result<TokenStream> {
    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(
            &input.ident,
            "Settings can only be derived for structs",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(Error::new_spanned(
            &input.ident,
            "Settings requires named fields",
        ));
    };

    let data_crate = member_crate("bevykit_data");
    let mut storage_key = format!("{}.json", crate::action::to_snake_case(&input.ident.to_string()));
    for attribute in &input.attrs {
        if !attribute.path().is_ident("settings") {
            continue;
        }
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("key") {
                storage_key = meta.value()?.parse::<LitStr>()?.value();
                Ok(())
            } else {
                Err(meta.error("expected `key = \"...\"`"))
            }
        })?;
    }

    let mut checks = Vec::new();
    for field in &fields.named {
        let ident = field.ident.as_ref().expect("named field");
        let name = ident.to_string();
        for attribute in &field.attrs {
            if !attribute.path().is_ident("setting") {
                continue;
            }
            attribute.parse_nested_meta(|meta| {
                if meta.path.is_ident("range") {
                    let range: Expr = meta.value()?.parse()?;
                    checks.push(quote! {
                        #data_crate::settings::clamp_to_range(
                            &mut self.#ident,
                            #range,
                            #name,
                            &mut issues,
                        );
                    });
                    Ok(())
                } else if meta.path.is_ident("validate") {
                    let validator: Path = meta.value()?.parse()?;
                    checks.push(quote! {
                        #data_crate::settings::apply_validator(
                            &mut self.#ident,
                            #validator,
                            #name,
                            &mut issues,
                        );
                    });
                    Ok(())
                } else {
                    Err(meta.error("expected `range = a..=b` or `validate = path`"))
                }
            })?;
        }
    }

    let ident = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #data_crate::settings::Settings for #ident #type_generics #where_clause {
            const STORAGE_KEY: &'static str = #storage_key;

            fn sanitize(&mut self) -> ::std::vec::Vec<#data_crate::settings::SettingIssue> {
                #[allow(unused_mut)]
                let mut issues = ::std::vec::Vec::new();
                #(#checks)*
                issues
            }
        }
    })
}
