//! `#[derive(ContentDefinition)]`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields, LitStr};

use crate::action::to_snake_case;
use crate::paths::member_crate;

pub fn derive(input: DeriveInput) -> syn::Result<TokenStream> {
    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(
            &input.ident,
            "ContentDefinition can only be derived for structs",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(Error::new_spanned(
            &input.ident,
            "ContentDefinition requires named fields",
        ));
    };

    let mut kind = to_snake_case(&input.ident.to_string());
    for attribute in &input.attrs {
        if !attribute.path().is_ident("content") {
            continue;
        }
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("kind") {
                kind = meta.value()?.parse::<LitStr>()?.value();
                Ok(())
            } else {
                Err(meta.error("expected `kind = \"...\"`"))
            }
        })?;
    }

    let data_crate = member_crate("bevykit_data");
    let mut id_field = None;
    let mut references = Vec::new();
    for field in &fields.named {
        let ident = field.ident.as_ref().expect("named field");
        let mut is_id = false;
        for attribute in &field.attrs {
            if !attribute.path().is_ident("content") {
                continue;
            }
            attribute.parse_nested_meta(|meta| {
                if meta.path.is_ident("id") {
                    is_id = true;
                    Ok(())
                } else if meta.path.is_ident("reference") {
                    let name = ident.to_string();
                    references.push(quote! {
                        #data_crate::content::CollectReferences::collect_references(
                            &self.#ident,
                            #name,
                            references,
                        );
                    });
                    Ok(())
                } else {
                    Err(meta.error("expected `id` or `reference`"))
                }
            })?;
        }
        if is_id || (id_field.is_none() && ident == "id") {
            id_field = Some(ident.clone());
        }
    }
    let Some(id_field) = id_field else {
        return Err(Error::new_spanned(
            &input.ident,
            "ContentDefinition requires an `id: ContentId<Self>` field or a field marked `#[content(id)]`",
        ));
    };

    let ident = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #data_crate::content::ContentDefinition for #ident #type_generics #where_clause {
            const KIND: &'static str = #kind;

            fn id(&self) -> &#data_crate::content::ContentId<Self> {
                &self.#id_field
            }

            fn references(
                &self,
                references: &mut ::std::vec::Vec<#data_crate::content::ContentReference>,
            ) {
                let _ = &references;
                #(#references)*
            }
        }
    })
}
