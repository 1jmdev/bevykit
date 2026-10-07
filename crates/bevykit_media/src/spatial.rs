//! Positional sound.
//!
//! Instances played with [`PlayCue::at_entity`](crate::playback::PlayCue::at_entity) copy the
//! followed entity's position every frame. When that entity despawns, the instance follows
//! its [`SourceLost`] policy. [`Audio::listener`](crate::playback::Audio::listener) attaches
//! the [`SpatialListener`](bevy::audio::SpatialListener) to a camera.

use bevy::prelude::*;

use crate::playback::Fade;

/// What a sound following an entity does when that entity despawns.
#[derive(Clone, Copy, PartialEq, Debug, Default, Reflect)]
pub enum SourceLost {
    /// Stop immediately.
    #[default]
    Stop,
    /// Fade out over the given seconds, then stop.
    FadeOut(f32),
    /// Keep playing at the last known position.
    KeepPosition,
}

/// Makes a sound instance follow an entity.
#[derive(Component, Clone, Copy, Debug)]
pub struct FollowEntity {
    /// The followed entity.
    pub target: Entity,
    /// What happens when the target despawns.
    pub lost: SourceLost,
}

pub(crate) fn follow_sources(
    mut sounds: Query<(Entity, &FollowEntity, &mut Transform, Option<&Fade>)>,
    targets: Query<&GlobalTransform>,
    mut commands: Commands,
) {
    for (entity, follow, mut transform, fade) in &mut sounds {
        if let Ok(target) = targets.get(follow.target) {
            *transform = target.compute_transform();
            continue;
        }
        let mut sound = commands.entity(entity);
        match follow.lost {
            SourceLost::Stop => sound.despawn(),
            SourceLost::FadeOut(seconds) => {
                sound
                    .remove::<FollowEntity>()
                    .insert(Fade::out(fade.map_or(1.0, Fade::value), seconds));
            }
            SourceLost::KeepPosition => {
                sound.remove::<FollowEntity>();
            }
        }
    }
}
