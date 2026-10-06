#![doc = include_str!("../README.md")]

extern crate self as bevykit_data;

pub mod save;
pub mod settings;
pub mod storage;

/// Commonly used items.
pub mod prelude {
    pub use crate::save::{
        AutosavePolicy, KitSavePlugin, LoadCompleted, SaveAppExt, SaveCompleted, SaveData,
        SaveError, SaveId, SaveIds, SaveSlot, Saves,
    };
    pub use crate::settings::{KitSettingsPlugin, PersistStatus, Settings, SettingsStore};
    pub use crate::storage::{FileStorage, KitStorage, MemoryStorage, StorageBackend};
}
