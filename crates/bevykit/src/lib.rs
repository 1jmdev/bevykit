#![doc = include_str!("../README.md")]

pub use bevykit_core as core;
#[cfg(any(
    feature = "assets",
    feature = "content",
    feature = "save",
    feature = "settings",
    feature = "localization"
))]
pub use bevykit_data as data;
#[cfg(feature = "input")]
pub use bevykit_input as input;
#[cfg(feature = "ui")]
pub use bevykit_ui as ui;

pub use bevykit_macros::*;

/// Everything a game typically needs, in one import alongside `bevy::prelude::*`.
pub mod prelude {
    pub use bevykit_core::prelude::*;
    #[cfg(any(
        feature = "assets",
        feature = "content",
        feature = "save",
        feature = "settings",
        feature = "localization"
    ))]
    pub use bevykit_data::prelude::*;
    #[cfg(feature = "input")]
    pub use bevykit_input::prelude::*;
    #[cfg(feature = "ui")]
    pub use bevykit_ui::prelude::*;

    pub use crate::KitPlugins;
}

use bevy::app::{PluginGroup, PluginGroupBuilder};

/// The non-generic bevykit plugins enabled by Cargo features.
///
/// Generic plugins, such as `KitInputPlugin<GameAction>` and `KitSettingsPlugin<GameSettings>`,
/// are added separately because they depend on the game's own types.
#[derive(Default)]
pub struct KitPlugins;

impl PluginGroup for KitPlugins {
    fn build(self) -> PluginGroupBuilder {
        let group = PluginGroupBuilder::start::<Self>().add(bevykit_core::KitCorePlugin);
        #[cfg(feature = "ui")]
        let group = group.add(bevykit_ui::KitUiPlugin::default());
        #[cfg(feature = "assets")]
        let group = group.add(bevykit_data::KitAssetsPlugin);
        #[cfg(feature = "content")]
        let group = group.add(bevykit_data::KitContentPlugin);
        #[cfg(feature = "save")]
        let group = group.add(bevykit_data::save::KitSavePlugin::default());
        group
    }
}
