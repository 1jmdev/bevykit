#![doc = include_str!("../README.md")]

pub mod deadline;
pub mod display;
pub mod key;
pub mod pause;
pub mod schedule;
pub mod scope;
pub mod task;

use bevy::prelude::*;

/// Commonly used items.
pub mod prelude {
    pub use crate::KitCorePlugin;
    pub use crate::deadline::{
        ClockChangePolicy, Deadline, DeadlineExpired, DeadlineTimer, Deadlines, WallClock,
        WallTime,
    };
    pub use crate::display::SafeAreaInsets;
    pub use crate::key::{IntoKey, Key};
    pub use crate::pause::{PausePolicy, PauseReason, PauseState, gameplay_paused, gameplay_running};
    pub use crate::schedule::KitSystems;
    pub use crate::scope::{OwnedBy, Scope, ScopeClosing, ScopeCommandsExt, ScopeOwned, Scopes};
    pub use crate::task::{KitTasks, TaskPoolKind};
}

/// Installs the shared foundations: schedule ordering, scopes, scoped tasks, pause control,
/// deadlines, and display information.
///
/// Every other bevykit plugin adds this plugin automatically if it is missing, so games only
/// need to add it explicitly to change its configuration.
#[derive(Default)]
pub struct KitCorePlugin;

impl Plugin for KitCorePlugin {
    fn build(&self, app: &mut App) {
        schedule::configure_sets(app);

        app.init_resource::<pause::PauseState>()
            .init_resource::<pause::PausePolicy>()
            .init_resource::<deadline::Deadlines>()
            .init_resource::<display::SafeAreaInsets>()
            .register_type::<scope::Scope>()
            .register_type::<scope::OwnedBy>()
            .register_type::<scope::ScopeOwned>()
            .register_type::<pause::PauseState>()
            .register_type::<display::SafeAreaInsets>()
            .add_systems(
                PreUpdate,
                (deadline::refresh_deadlines, pause::apply_pause_to_time)
                    .in_set(schedule::KitSystems::Platform),
            )
            .add_systems(
                Last,
                (task::poll_pending_tasks, deadline::report_expired_deadlines)
                    .in_set(schedule::KitSystems::Cleanup),
            );
    }
}

/// Adds [`KitCorePlugin`] to the app unless it is already present.
///
/// Called by every bevykit plugin so that each module works on its own.
pub fn ensure_core(app: &mut App) {
    if !app.is_plugin_added::<KitCorePlugin>() {
        app.add_plugins(KitCorePlugin);
    }
}
