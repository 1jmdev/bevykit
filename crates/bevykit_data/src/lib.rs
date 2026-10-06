#![doc = include_str!("../README.md")]

extern crate self as bevykit_data;

pub mod localization;
pub mod save;
pub mod settings;
pub mod storage;

/// Commonly used items.
pub mod prelude {
    pub use crate::localization::{
        KitLocalizationPlugin, LanguageId, Locale, LocalizedKey, LocalizedText, TextArg,
        TextDirection,
    };
    pub use crate::save::{
        AutosavePolicy, KitSavePlugin, LoadCompleted, SaveAppExt, SaveCompleted, SaveData,
        SaveError, SaveId, SaveIds, SaveSlot, Saves,
    };
    pub use crate::settings::{KitSettingsPlugin, PersistStatus, Settings, SettingsStore};
    pub use crate::storage::{FileStorage, KitStorage, MemoryStorage, StorageBackend};
    pub use crate::tr;
}
