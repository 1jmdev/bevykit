//! Compact, hashable identifiers that can be built from strings or from game-defined types.
//!
//! Many bevykit registries (audio cues, panels, focus targets, input contexts) are keyed by
//! values that the game chooses. A game may prefer string identifiers such as `"confirm"` or
//! its own enums such as `SoundId::Confirm`. [`Key`] unifies both forms into a 128-bit value
//! that is cheap to copy, compare, and hash.
//!
//! String-like values (`&str`, `String`, `Box<str>`, `Arc<str>`, `Cow<str>`) share a namespace,
//! so `"confirm"` and `String::from("confirm")` produce the same key. Every other type receives
//! its own namespace, derived from its [`TypeId`], so equal hashes of different types never
//! collide by construction.

use core::any::TypeId;
use core::fmt;
use core::hash::{Hash, Hasher};
use std::borrow::Cow;
use std::sync::Arc;

use bevy::reflect::Reflect;

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The namespace shared by every string-like value.
const STRING_NAMESPACE: u64 = 0;

/// A deterministic FNV-1a hasher.
///
/// Unlike the standard library's randomized hasher, this produces the same value on every run
/// and on every platform, which allows keys to be computed in `const` contexts.
#[derive(Clone, Copy, Debug)]
pub struct StableHasher {
    state: u64,
}

impl StableHasher {
    /// Creates a hasher with the FNV-1a offset basis.
    pub const fn new() -> Self {
        Self {
            state: FNV_OFFSET_BASIS,
        }
    }

    /// Feeds a byte slice into the hasher in a `const` context.
    pub const fn write_const(mut self, bytes: &[u8]) -> Self {
        let mut index = 0;
        while index < bytes.len() {
            self.state ^= bytes[index] as u64;
            self.state = self.state.wrapping_mul(FNV_PRIME);
            index += 1;
        }
        self
    }

    /// Returns the current hash value in a `const` context.
    pub const fn finish_const(self) -> u64 {
        self.state
    }
}

impl Default for StableHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher for StableHasher {
    fn finish(&self) -> u64 {
        self.state
    }

    fn write(&mut self, bytes: &[u8]) {
        *self = self.write_const(bytes);
    }
}

/// Computes the stable hash of a string exactly as `str`'s [`Hash`] implementation would feed
/// it into a [`StableHasher`].
pub const fn stable_str_hash(value: &str) -> u64 {
    StableHasher::new()
        .write_const(value.as_bytes())
        .write_const(&[0xff])
        .finish_const()
}

/// A compact identifier built from a string or from any hashable game-defined value.
///
/// ```
/// use bevykit_core::key::{IntoKey, Key};
///
/// #[derive(Hash, Debug)]
/// enum SoundId {
///     Confirm,
/// }
///
/// assert_eq!(Key::from_static("confirm"), "confirm".into_key());
/// assert_eq!("confirm".into_key(), String::from("confirm").into_key());
/// assert_ne!(SoundId::Confirm.into_key(), "Confirm".into_key());
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Reflect)]
#[reflect(opaque)]
#[reflect(Clone, PartialEq, Hash, Debug)]
pub struct Key {
    namespace: u64,
    hash: u64,
}

impl Key {
    /// Builds a key from a string in a `const` context.
    pub const fn from_static(value: &'static str) -> Self {
        Self {
            namespace: STRING_NAMESPACE,
            hash: stable_str_hash(value),
        }
    }

    /// Builds a key from any string slice.
    pub const fn from_str(value: &str) -> Self {
        Self {
            namespace: STRING_NAMESPACE,
            hash: stable_str_hash(value),
        }
    }

    /// Builds a key from an arbitrary hashable value, namespaced by its type.
    pub fn of<T: Hash + 'static>(value: &T) -> Self {
        let mut hasher = StableHasher::new();
        value.hash(&mut hasher);
        Self {
            namespace: type_namespace::<T>(),
            hash: hasher.finish(),
        }
    }

    /// Returns `true` when this key was built from a string-like value.
    pub const fn is_string(&self) -> bool {
        self.namespace == STRING_NAMESPACE
    }

    /// Returns the raw 128-bit representation, useful for diagnostics or serialization.
    pub const fn to_bits(&self) -> u128 {
        ((self.namespace as u128) << 64) | self.hash as u128
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Key({:032x})", self.to_bits())
    }
}

fn type_namespace<T: 'static>() -> u64 {
    let type_id = TypeId::of::<T>();
    let string_types = [
        TypeId::of::<&'static str>(),
        TypeId::of::<String>(),
        TypeId::of::<Box<str>>(),
        TypeId::of::<Arc<str>>(),
        TypeId::of::<Cow<'static, str>>(),
        TypeId::of::<&'static String>(),
    ];
    if string_types.contains(&type_id) {
        return STRING_NAMESPACE;
    }
    let mut hasher = StableHasher::new();
    type_id.hash(&mut hasher);
    // Reserve zero for strings even in the astronomically unlikely event of a collision.
    hasher.finish().max(1)
}

/// Conversion into a [`Key`].
///
/// Implemented for every `Hash + 'static` value, so game enums, strings, integers, and tuples
/// can all be used wherever bevykit accepts an identifier.
pub trait IntoKey {
    /// Converts the value into a [`Key`].
    fn into_key(self) -> Key;
}

impl<T: Hash + 'static> IntoKey for T {
    fn into_key(self) -> Key {
        Key::of(&self)
    }
}

/// Declares a strongly typed identifier wrapping a [`Key`].
///
/// The generated type converts from any [`IntoKey`] value and can be constructed in `const`
/// contexts from string literals.
///
/// ```
/// bevykit_core::define_key!(
///     /// Identifies a panel.
///     PanelId
/// );
///
/// const SETTINGS: PanelId = PanelId::from_static("settings");
/// assert_eq!(SETTINGS, PanelId::new("settings"));
/// ```
#[macro_export]
macro_rules! define_key {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, ::bevy::reflect::Reflect)]
        #[reflect(Clone, PartialEq, Hash, Debug)]
        pub struct $name(pub $crate::key::Key);

        impl $name {
            /// Builds the identifier from a string literal in a `const` context.
            pub const fn from_static(value: &'static str) -> Self {
                Self($crate::key::Key::from_static(value))
            }

            /// Builds the identifier from any value convertible into a key.
            pub fn new(value: impl $crate::key::IntoKey) -> Self {
                Self(value.into_key())
            }

            /// Returns the underlying key.
            pub const fn key(&self) -> $crate::key::Key {
                self.0
            }
        }

        impl From<$crate::key::Key> for $name {
            fn from(key: $crate::key::Key) -> Self {
                Self(key)
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Hash)]
    enum Example {
        Alpha,
    }

    #[test]
    fn const_hash_matches_runtime_hash() {
        let mut hasher = StableHasher::new();
        "settings.title".hash(&mut hasher);
        assert_eq!(hasher.finish(), stable_str_hash("settings.title"));
    }

    #[test]
    fn string_forms_share_namespace() {
        let expected = Key::from_static("alpha");
        assert_eq!("alpha".into_key(), expected);
        assert_eq!(String::from("alpha").into_key(), expected);
        assert_eq!(Arc::<str>::from("alpha").into_key(), expected);
        assert_eq!(Cow::<'static, str>::Borrowed("alpha").into_key(), expected);
        assert!(expected.is_string());
    }

    #[test]
    fn typed_keys_do_not_collide_with_strings() {
        let typed = Example::Alpha.into_key();
        assert!(!typed.is_string());
        assert_ne!(typed, Key::of(&0_u32));
    }
}
