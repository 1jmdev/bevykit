//! Application lifecycle, haptics, and rendering quality profiles.
//!
//! Mobile platforms suspend applications without warning. When the application is about to
//! move to the background, bevykit runs the [`OnBackground`] schedule, holds the
//! [`PauseReason::BACKGROUND`] pause, and (through the input module) cancels held input and
//! active gestures. When it returns, the [`OnForeground`] schedule runs.
//!
//! ```ignore
//! app.add_systems(OnBackground, request_checkpoint)
//!     .add_systems(OnForeground, refresh_platform_state);
//! ```
//!
//! Suspension may leave too little time for a write started at that moment, so games should
//! checkpoint proactively and use the background hook only as a final flush. bevykit's save
//! and settings modules write synchronously when suspension begins.

use std::borrow::Cow;
use std::sync::Arc;

use bevy::ecs::message::MessageCursor;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::window::AppLifecycle;

use crate::pause::{PauseReason, PauseState};
use crate::schedule::KitSystems;

/// Runs when the application is about to move to the background.
#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OnBackground;

/// Runs when the application returns to the foreground.
#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OnForeground;

/// Whether the application is in the foreground.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[reflect(Resource)]
pub struct LifecycleState {
    /// `true` while the application is in the foreground.
    pub foreground: bool,
    /// Pause gameplay while in the background.
    pub pause_in_background: bool,
}

impl Default for LifecycleState {
    fn default() -> Self {
        Self {
            foreground: true,
            pause_in_background: true,
        }
    }
}

pub(crate) fn track_lifecycle(world: &mut World, mut cursor: Local<MessageCursor<AppLifecycle>>) {
    let events: Vec<AppLifecycle> = match world.get_resource::<Messages<AppLifecycle>>() {
        Some(messages) => cursor.read(messages).copied().collect(),
        None => return,
    };
    for event in events {
        let foreground = match event {
            AppLifecycle::WillSuspend | AppLifecycle::Suspended => false,
            AppLifecycle::WillResume | AppLifecycle::Running => true,
            AppLifecycle::Idle => continue,
        };
        let mut state = world.resource_mut::<LifecycleState>();
        if state.foreground == foreground {
            continue;
        }
        state.foreground = foreground;
        let pause = state.pause_in_background;
        if pause {
            world
                .resource_mut::<PauseState>()
                .set(PauseReason::BACKGROUND, !foreground);
        }
        let schedule = if foreground {
            OnForeground.intern()
        } else {
            OnBackground.intern()
        };
        // Running a schedule that has no systems is cheap, but a missing one is an error.
        let _ = world.try_run_schedule(schedule);
    }
}

/// A haptic feedback pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
pub enum HapticPattern {
    /// A light tap, such as a selection change.
    Selection,
    /// A light impact.
    Light,
    /// A medium impact.
    Medium,
    /// A heavy impact.
    Heavy,
    /// A successful action.
    Success,
    /// A warning.
    Warning,
    /// A failed action.
    Error,
}

/// Plays haptic patterns on a device.
pub trait HapticBackend: Send + Sync + 'static {
    /// Plays a pattern. Called on the main schedule; implementations should not block.
    fn play(&self, pattern: HapticPattern);
}

/// Haptic output. Without a backend, requests are accepted and ignored, so games can request
/// haptics unconditionally.
#[derive(Resource, Clone)]
pub struct Haptics {
    backend: Option<Arc<dyn HapticBackend>>,
    /// Whether haptics are enabled, typically bound to a player setting.
    pub enabled: bool,
}

impl Default for Haptics {
    fn default() -> Self {
        Self {
            backend: None,
            enabled: true,
        }
    }
}

impl Haptics {
    /// Installs the platform backend.
    pub fn set_backend(&mut self, backend: impl HapticBackend) {
        self.backend = Some(Arc::new(backend));
    }

    /// Returns `true` if a backend is installed.
    pub fn is_available(&self) -> bool {
        self.backend.is_some()
    }

    /// Plays a pattern if haptics are enabled and available.
    pub fn play(&self, pattern: HapticPattern) {
        if self.enabled
            && let Some(backend) = &self.backend
        {
            backend.play(pattern);
        }
    }
}

/// Names a rendering quality level.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Reflect, serde::Serialize, serde::Deserialize)]
pub struct QualityLevel(pub Cow<'static, str>);

#[allow(non_upper_case_globals)]
impl QualityLevel {
    /// Lowest quality, for constrained devices.
    pub const Low: Self = Self(Cow::Borrowed("low"));
    /// Balanced quality.
    pub const Medium: Self = Self(Cow::Borrowed("medium"));
    /// High quality.
    pub const High: Self = Self(Cow::Borrowed("high"));

    /// Creates a custom level.
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        Self(name.into())
    }
}

type QualityApplier = Arc<dyn Fn(&mut World) + Send + Sync>;

/// Game-declared quality profiles. Each profile is a function applying its choices (render
/// scale, shadows, effects) to the world; bevykit only stores and applies them.
#[derive(Resource, Default)]
pub struct QualityProfiles {
    profiles: HashMap<QualityLevel, QualityApplier>,
    order: Vec<QualityLevel>,
    active: Option<QualityLevel>,
    pending: Option<QualityLevel>,
}

impl QualityProfiles {
    /// Registers or replaces the profile for a level.
    pub fn register(
        &mut self,
        level: QualityLevel,
        apply: impl Fn(&mut World) + Send + Sync + 'static,
    ) -> &mut Self {
        if !self.order.contains(&level) {
            self.order.push(level.clone());
        }
        self.profiles.insert(level, Arc::new(apply));
        self
    }

    /// Requests a level. It is applied at the start of the next frame.
    pub fn apply(&mut self, level: QualityLevel) {
        if self.active.as_ref() != Some(&level) {
            self.pending = Some(level);
        }
    }

    /// Returns the active level.
    pub fn active(&self) -> Option<&QualityLevel> {
        self.active.as_ref()
    }

    /// Returns the registered levels in registration order.
    pub fn levels(&self) -> &[QualityLevel] {
        &self.order
    }
}

pub(crate) fn apply_quality(world: &mut World) {
    let pending = {
        let Some(mut profiles) = world.get_resource_mut::<QualityProfiles>() else {
            return;
        };
        let Some(level) = profiles.pending.take() else {
            return;
        };
        match profiles.profiles.get(&level).cloned() {
            Some(apply) => {
                profiles.active = Some(level);
                apply
            }
            None => {
                warn!("Quality level {:?} is not registered", level.0);
                return;
            }
        }
    };
    pending(world);
}

pub(crate) fn build(app: &mut App) {
    app.add_message::<AppLifecycle>()
        .init_resource::<LifecycleState>()
        .init_resource::<Haptics>()
        .init_resource::<QualityProfiles>()
        .init_schedule(OnBackground)
        .init_schedule(OnForeground)
        .add_systems(
            PreUpdate,
            (
                track_lifecycle.before(crate::pause::apply_pause_to_time),
                apply_quality,
            )
                .in_set(KitSystems::Platform),
        );
}
