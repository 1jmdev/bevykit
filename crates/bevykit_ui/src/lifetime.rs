//! Temporary entities that expire on their own.

use bevy::prelude::*;
use bevykit_core::tween::TimeDomain;

/// What happens to an entity when its [`Lifetime`] ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum ExpiryAction {
    /// Despawn the entity and its children.
    #[default]
    Despawn,
    /// Hide the entity and remove the lifetime, so the game can reuse it from a pool.
    Hide,
}

/// Expires an entity after a duration.
#[derive(Component, Clone, Copy, Debug, Reflect)]
#[reflect(Component)]
pub struct Lifetime {
    /// Seconds remaining.
    pub remaining: f32,
    /// What happens on expiry.
    pub action: ExpiryAction,
    /// Which clock counts down.
    pub domain: TimeDomain,
}

impl Lifetime {
    /// Despawns after `seconds` of presentation time.
    pub fn seconds(seconds: f32) -> Self {
        Self {
            remaining: seconds,
            action: ExpiryAction::Despawn,
            domain: TimeDomain::Presentation,
        }
    }

    /// Counts gameplay time instead, so the lifetime pauses with gameplay.
    pub fn gameplay(mut self) -> Self {
        self.domain = TimeDomain::Gameplay;
        self
    }

    /// Hides instead of despawning, for pooled effects.
    pub fn hide_on_expiry(mut self) -> Self {
        self.action = ExpiryAction::Hide;
        self
    }
}

pub(crate) fn expire_lifetimes(
    real: Res<Time<Real>>,
    virtual_time: Res<Time<Virtual>>,
    mut lifetimes: Query<(Entity, &mut Lifetime)>,
    mut commands: Commands,
) {
    for (entity, mut lifetime) in &mut lifetimes {
        lifetime.remaining -= match lifetime.domain {
            TimeDomain::Presentation => real.delta_secs(),
            TimeDomain::Gameplay => virtual_time.delta_secs(),
        };
        if lifetime.remaining > 0.0 {
            continue;
        }
        match lifetime.action {
            ExpiryAction::Despawn => {
                commands.entity(entity).try_despawn();
            }
            ExpiryAction::Hide => {
                commands
                    .entity(entity)
                    .insert(Visibility::Hidden)
                    .remove::<Lifetime>();
            }
        }
    }
}
