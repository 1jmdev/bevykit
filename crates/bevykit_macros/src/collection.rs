//! `#[derive(AssetCollection)]`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields, LitStr};

use crate::paths::member_crate;

enum Source {
    Path(LitStr),
    Paths(Vec<LitStr>),
    Default,
}

pub fn derive(input: DeriveInput) -> syn::Result<TokenStream> {
    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(
            &input.ident,
            "AssetCollection can only be derived for structs",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(Error::new_spanned(
            &input.ident,
            "AssetCollection requires named fields",
        ));
    };

    let mut initializers = Vec::new();
    let mut handles = Vec::new();
    for field in &fields.named {
        let ident = field.ident.as_ref().expect("named field");
        let mut source = Source::Default;
        for attribute in &field.attrs {
            if !attribute.path().is_ident("asset") {
                continue;
            }
            attribute.parse_nested_meta(|meta| {
                if meta.path.is_ident("path") {
                    source = Source::Path(meta.value()?.parse()?);
                    Ok(())
                } else if meta.path.is_ident("paths") {
                    let content;
                    syn::parenthesized!(content in meta.input);
                    let paths = content.parse_terminated(<LitStr as syn::parse::Parse>::parse, syn::Token![,])?;
                    source = Source::Paths(paths.into_iter().collect());
                    Ok(())
                } else {
                    Err(meta.error("expected `path = \"...\"` or `paths(\"...\", ...)`"))
                }
            })?;
        }
        match source {
            Source::Path(path) => {
                initializers.push(quote!(#ident: server.load(#path)));
                handles.push(quote!(handles.push(self.#ident.clone().untyped());));
            }
            Source::Paths(paths) => {
                initializers.push(quote!(#ident: ::std::vec![#(server.load(#paths)),*]));
                handles.push(quote! {
                    handles.extend(self.#ident.iter().map(|handle| handle.clone().untyped()));
                });
            }
            Source::Default => {
                initializers.push(quote!(#ident: ::core::default::Default::default()));
            }
        }
    }

    let data_crate = member_crate("bevykit_data");
    let ident = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #data_crate::assets::AssetCollection for #ident #type_generics #where_clause {
            fn load(server: &::bevy::asset::AssetServer) -> Self {
                Self {
                    #(#initializers,)*
                }
            }

            fn handles(&self) -> ::std::vec::Vec<::bevy::asset::UntypedHandle> {
                #[allow(unused_mut)]
                let mut handles = ::std::vec::Vec::new();
                #(#handles)*
                handles
            }
        }
    })
}
