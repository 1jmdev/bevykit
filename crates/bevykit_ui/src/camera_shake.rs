//! Camera impulses: short, decaying shakes applied on top of the game's camera transform.
//!
//! The shake offset is removed before the game's systems run each frame and reapplied
//! afterwards, so camera controllers never see or accumulate it.

use bevy::prelude::*;

/// A decaying shake on a camera (or any entity with a [`Transform`]).
#[derive(Component, Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct CameraShake {
    /// Current intensity in `0.0..=1.0`. Decays over time; add to it for new impulses.
    pub trauma: f32,
    /// Trauma lost per second.
    pub decay: f32,
    /// Largest translation offset, in world units.
    pub max_offset: f32,
    /// Largest rotation offset, in radians.
    pub max_angle: f32,
    /// Shake frequency in hertz.
    pub frequency: f32,
    applied_offset: Vec3,
    applied_rotation: Quat,
    time: f32,
}

impl CameraShake {
    /// Creates a shake with moderate defaults.
    pub fn new() -> Self {
        Self {
            trauma: 0.0,
            decay: 1.5,
            max_offset: 0.3,
            max_angle: 0.03,
            frequency: 18.0,
            applied_offset: Vec3::ZERO,
            applied_rotation: Quat::IDENTITY,
            time: 0.0,
        }
    }

    /// Adds an impulse; trauma saturates at one.
    pub fn add_trauma(&mut self, amount: f32) {
        self.trauma = (self.trauma + amount).clamp(0.0, 1.0);
    }
}

fn noise(seed: f32, time: f32) -> f32 {
    // A sum of incommensurate sines: smooth, cheap, and never repeating noticeably.
    ((time * 1.0 + seed).sin() + (time * 2.3 + seed * 1.7).sin() * 0.5) / 1.5
}

pub(crate) fn remove_shake_offset(mut shakes: Query<(&mut CameraShake, &mut Transform)>) {
    for (mut shake, mut transform) in &mut shakes {
        if shake.applied_offset == Vec3::ZERO && shake.applied_rotation == Quat::IDENTITY {
            continue;
        }
        transform.translation -= shake.applied_offset;
        transform.rotation = shake.applied_rotation.inverse() * transform.rotation;
        shake.applied_offset = Vec3::ZERO;
        shake.applied_rotation = Quat::IDENTITY;
    }
}

pub(crate) fn apply_shake_offset(
    time: Res<Time<Real>>,
    settings: Res<crate::feedback::FeedbackSettings>,
    mut shakes: Query<(&mut CameraShake, &mut Transform)>,
) {
    for (mut shake, mut transform) in &mut shakes {
        shake.trauma = (shake.trauma - shake.decay * time.delta_secs()).max(0.0);
        if shake.trauma == 0.0 || settings.reduce_motion {
            continue;
        }
        shake.time += time.delta_secs() * shake.frequency;
        let strength = shake.trauma * shake.trauma;
        let offset = Vec3::new(noise(1.0, shake.time), noise(7.0, shake.time), 0.0)
            * shake.max_offset
            * strength;
        let rotation = Quat::from_rotation_z(noise(13.0, shake.time) * shake.max_angle * strength);
        let offset = transform.rotation * offset;
        transform.translation += offset;
        transform.rotation = rotation * transform.rotation;
        shake.applied_offset = offset;
        shake.applied_rotation = rotation;
    }
}
