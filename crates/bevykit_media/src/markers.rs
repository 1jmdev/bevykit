//! Named clip markers and completion events.
//!
//! [`AnimationMarkers`] maps marker names to clip times. A clip played with
//! [`PlayAnimation::notify_at`](crate::controller::PlayAnimation::notify_at) triggers its event
//! once per pass over the marker, including when a single frame skips over the marker time.
//!
//! ```ignore
//! fn define_markers(mut markers: ResMut<AnimationMarkers>) {
//!     markers.insert("Punch", "contact", 0.32);
//! }
//!
//! fn punch(mut animations: Animations, character: Single<Entity, With<Player>>) {
//!     let character = *character;
//!     animations
//!         .on(character)
//!         .play("Punch")
//!         .notify_at("contact", InteractionContact { character });
//! }
//! ```

use bevy::animation::AnimationPlayer;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::clips::AnimationLibrary;
use crate::controller::AnimationPlayback;

/// Marker times in seconds, by clip name and marker name.
#[derive(Resource, Default, Debug)]
pub struct AnimationMarkers {
    clips: HashMap<String, HashMap<String, f32>>,
}

impl AnimationMarkers {
    /// Defines a marker of a clip.
    pub fn insert(
        &mut self,
        clip: impl Into<String>,
        marker: impl Into<String>,
        seconds: f32,
    ) -> &mut Self {
        self.clips
            .entry(clip.into())
            .or_default()
            .insert(marker.into(), seconds);
        self
    }

    /// Returns the time of a marker.
    pub fn get(&self, clip: &str, marker: &str) -> Option<f32> {
        self.clips.get(clip)?.get(marker).copied()
    }
}

/// Triggered on a character when a clip played with `.once()` reaches its end.
#[derive(EntityEvent, Clone, Debug)]
pub struct AnimationFinished {
    /// The character.
    pub entity: Entity,
    /// The clip name.
    pub clip: String,
}

pub(crate) fn track_playback(
    time: Res<Time>,
    mut characters: Query<(Entity, &AnimationLibrary, &mut AnimationPlayback)>,
    mut players: Query<&mut AnimationPlayer>,
    mut commands: Commands,
) {
    for (character, library, mut playback) in &mut characters {
        let Ok(mut player) = players.get_mut(library.player()) else {
            continue;
        };
        for layer in playback.layers.iter_mut().flatten() {
            if layer.finished {
                continue;
            }
            let Some(active) = player.animation_mut(layer.node) else {
                layer.finished = true;
                continue;
            };
            if layer.weight < 1.0 {
                layer.weight = (layer.weight + time.delta_secs() / layer.blend_in).min(1.0);
                active.set_weight(layer.weight);
            }
            let position = active.completions() as f32 * layer.duration
                + if active.is_finished() {
                    0.0
                } else {
                    active.seek_time()
                };
            for (marker, notify) in &layer.markers {
                let passes = if layer.repeat && layer.duration > 0.0 {
                    ((position - marker) / layer.duration).floor()
                        - ((layer.position - marker) / layer.duration).floor()
                } else {
                    f32::from(layer.position < *marker && *marker <= position)
                };
                for _ in 0..passes as u32 {
                    notify(&mut commands);
                }
            }
            layer.position = position;
            layer.elapsed = active.elapsed();
            if active.is_finished() {
                layer.finished = true;
                commands.trigger(AnimationFinished {
                    entity: character,
                    clip: layer.clip.clone(),
                });
            }
        }
    }
}
