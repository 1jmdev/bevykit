#![doc = include_str!("../README.md")]

#[cfg(feature = "audio")]
pub mod bus;
#[cfg(feature = "animation")]
pub mod clips;
#[cfg(feature = "animation")]
pub mod controller;
#[cfg(feature = "audio")]
pub mod cue;
#[cfg(feature = "animation")]
pub mod markers;
#[cfg(feature = "audio")]
pub mod music;
#[cfg(feature = "audio")]
pub mod playback;
#[cfg(feature = "audio")]
pub mod policy;
#[cfg(feature = "scene")]
pub mod scene_bindings;
#[cfg(feature = "audio")]
pub mod spatial;

use bevy::prelude::*;

/// Commonly used items.
pub mod prelude {
    #[cfg(feature = "animation")]
    pub use crate::KitAnimationPlugin;
    #[cfg(feature = "audio")]
    pub use crate::KitAudioPlugin;
    #[cfg(feature = "scene")]
    pub use crate::KitScenePlugin;
    #[cfg(feature = "audio")]
    pub use crate::bus::{AudioBus, AudioMixer, BusState};
    #[cfg(feature = "animation")]
    pub use crate::clips::AnimationLibrary;
    #[cfg(feature = "animation")]
    pub use crate::controller::{AnimationPlayback, Animations};
    #[cfg(feature = "audio")]
    pub use crate::cue::{Cue, CueId, CueRegistry, VoiceStealing};
    #[cfg(feature = "animation")]
    pub use crate::markers::{AnimationFinished, AnimationMarkers};
    #[cfg(feature = "audio")]
    pub use crate::music::{MusicId, MusicPlayer, PlayingMusic, Playlist};
    #[cfg(feature = "audio")]
    pub use crate::playback::{Audio, Fade, PlayingCue};
    #[cfg(feature = "audio")]
    pub use crate::policy::{AudioPausePolicy, BusPausePolicy};
    #[cfg(feature = "scene")]
    pub use crate::scene_bindings::{
        SceneBindingError, SceneBindingErrorKind, SceneBindings, SceneNodes,
    };
    #[cfg(feature = "audio")]
    pub use crate::spatial::{FollowEntity, SourceLost};
}

/// Installs volume buses, cues, spatial sound, music, and the pause and lifecycle policy.
///
/// Requires Bevy's `AudioPlugin`, which `DefaultPlugins` includes.
#[cfg(feature = "audio")]
#[derive(Default)]
pub struct KitAudioPlugin;

#[cfg(feature = "audio")]
impl Plugin for KitAudioPlugin {
    fn build(&self, app: &mut App) {
        use bevykit_core::platform::OnBackground;
        use bevykit_core::schedule::KitSystems;

        bevykit_core::ensure_core(app);
        app.init_resource::<bus::AudioMixer>()
            .init_resource::<policy::AudioPausePolicy>()
            .init_resource::<cue::CueRegistry>()
            .init_resource::<music::MusicPlayer>()
            .add_systems(
                PostUpdate,
                (
                    spatial::follow_sources,
                    music::drive_music,
                    playback::advance_fades,
                    playback::apply_audio_output,
                )
                    .chain()
                    .in_set(KitSystems::Presentation),
            )
            .add_systems(OnBackground, playback::apply_audio_output);
    }
}

/// Installs scene bindings.
#[cfg(feature = "scene")]
#[derive(Default)]
pub struct KitScenePlugin;

#[cfg(feature = "scene")]
impl Plugin for KitScenePlugin {
    fn build(&self, app: &mut App) {
        bevykit_core::ensure_core(app);
        app.add_message::<scene_bindings::SceneBindingError>()
            .add_observer(scene_bindings::resolve_scene_bindings);
    }
}

/// Installs animation libraries, the [`Animations`](controller::Animations) parameter, and
/// markers. Adds [`KitScenePlugin`] if it is missing.
#[cfg(feature = "animation")]
#[derive(Default)]
pub struct KitAnimationPlugin;

#[cfg(feature = "animation")]
impl Plugin for KitAnimationPlugin {
    fn build(&self, app: &mut App) {
        use bevy::app::AnimationSystems;

        if !app.is_plugin_added::<KitScenePlugin>() {
            app.add_plugins(KitScenePlugin);
        }
        app.init_resource::<markers::AnimationMarkers>()
            .add_observer(clips::build_animation_library)
            .add_systems(PostUpdate, markers::track_playback.after(AnimationSystems));
    }
}
