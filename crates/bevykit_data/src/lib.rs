#![doc = include_str!("../README.md")]

extern crate self as bevykit_data;

pub mod assets;
pub mod localization;
pub mod save;
pub mod settings;
pub mod storage;

/// Commonly used items.
pub mod prelude {
    pub use crate::KitAssetsPlugin;
    pub use crate::assets::{
        AssetCollection, AssetGroup, AssetGroupFailed, AssetGroupReady, AssetGroups,
        GroupStatus, MissingAssetPolicy, asset_group_ready,
    };
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

use bevy::prelude::*;

/// Installs asset loading groups.
#[derive(Default)]
pub struct KitAssetsPlugin;

impl Plugin for KitAssetsPlugin {
    fn build(&self, app: &mut App) {
        bevykit_core::ensure_core(app);
        app.init_resource::<assets::AssetGroups>()
            .add_message::<assets::AssetGroupReady>()
            .add_message::<assets::AssetGroupFailed>()
            .add_systems(PreUpdate, assets::process_asset_groups);
    }
}
