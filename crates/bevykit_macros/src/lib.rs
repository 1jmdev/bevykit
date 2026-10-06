//! Derive macros for bevykit.
//!
//! These macros are re-exported by `bevykit` and its member crates; depend on those instead of
//! using this crate directly.

mod action;
mod paths;

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

/// Implements `Action` for a field-less enum, naming each variant in `snake_case`.
///
/// Use `#[action(name = "...")]` on a variant to choose the name used in saved bindings.
#[proc_macro_derive(KitAction, attributes(action))]
pub fn derive_kit_action(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    action::derive(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
