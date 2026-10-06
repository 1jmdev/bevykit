//! Derive macros for bevykit.
//!
//! These macros are re-exported by `bevykit` and its member crates; depend on those instead of
//! using this crate directly.

mod action;
mod collection;
mod content;
mod paths;
mod save;
mod settings;

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

/// Implements `Settings` for a struct with named fields.
///
/// - `#[settings(key = "name.json")]` chooses the storage record (default: the type name in
///   `snake_case` with a `.json` extension).
/// - `#[setting(range = a..=b)]` clamps a field into range.
/// - `#[setting(validate = path)]` checks a field with `fn(&mut T) -> Result<(), String>`.
#[proc_macro_derive(Settings, attributes(settings, setting))]
pub fn derive_settings(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    settings::derive(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Implements `SaveData` for a serializable type.
///
/// - `#[save(version = N)]` sets the current version (default 1).
/// - `#[save(name = "...")]` sets the entry name within save files (default: the type name in
///   `snake_case`).
#[proc_macro_derive(SaveData, attributes(save))]
pub fn derive_save_data(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    save::derive(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Implements `AssetCollection` for a struct of handles.
///
/// - `#[asset(path = "...")]` loads a `Handle<T>` field.
/// - `#[asset(paths("...", "..."))]` loads a `Vec<Handle<T>>` field.
/// - Fields without the attribute use `Default`.
#[proc_macro_derive(AssetCollection, attributes(asset))]
pub fn derive_asset_collection(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    collection::derive(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Implements `ContentDefinition` for a struct with named fields.
///
/// - The identifier is the field named `id`, or the field marked `#[content(id)]`.
/// - `#[content(reference)]` marks fields holding `ContentId`s (directly, in `Option`, or in
///   `Vec`) that must refer to existing definitions.
/// - `#[content(kind = "...")]` names the kind in error messages (default: the type name in
///   `snake_case`).
#[proc_macro_derive(ContentDefinition, attributes(content))]
pub fn derive_content_definition(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    content::derive(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
