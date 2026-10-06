//! Gameplay pause control that keeps presentation responsive.
//!
//! Pausing is reference-counted by reason: a pause menu, an open dialog, and the application
//! moving to the background can each hold a pause independently, and gameplay resumes only
//! when every reason has been released.
//!
//! While paused, [`Time<Virtual>`] is paused by default, which halts `FixedUpdate` and every
//! system that reads virtual time. Presentation systems should read [`Time<Real>`] or use
//! bevykit utilities configured for presentation time. Systems that must stop explicitly can
//! use the [`gameplay_running`] run condition.
//!
//! ```ignore
//! app.add_systems(FixedUpdate, simulate_game.run_if(gameplay_running));
//!
//! fn open_pause_menu(mut pause: ResMut<PauseState>) {
//!     pause.request(PauseReason::MENU);
//! }
//! ```

use std::borrow::Cow;

use bevy::platform::collections::HashSet;
use bevy::prelude::*;

/// Identifies why gameplay is paused.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Reflect)]
pub struct PauseReason(pub Cow<'static, str>);

impl PauseReason {
    /// A pause requested by a menu.
    pub const MENU: Self = Self::from_static("menu");
    /// A pause requested by a modal dialog.
    pub const DIALOG: Self = Self::from_static("dialog");
    /// A pause caused by the application losing focus or moving to the background.
    pub const BACKGROUND: Self = Self::from_static("background");
    /// A pause requested while content is loading.
    pub const LOADING: Self = Self::from_static("loading");

    /// Creates a reason from a static string.
    pub const fn from_static(name: &'static str) -> Self {
        Self(Cow::Borrowed(name))
    }

    /// Creates a reason from any string.
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        Self(name.into())
    }
}

impl From<&'static str> for PauseReason {
    fn from(value: &'static str) -> Self {
        Self::from_static(value)
    }
}

/// Configures how pausing affects time.
#[derive(Resource, Clone, Debug, Reflect)]
#[reflect(Resource, Debug)]
pub struct PausePolicy {
    /// Pause [`Time<Virtual>`] while any pause reason is held.
    pub pause_virtual_time: bool,
}

impl Default for PausePolicy {
    fn default() -> Self {
        Self {
            pause_virtual_time: true,
        }
    }
}

/// The set of reasons currently pausing gameplay.
#[derive(Resource, Default, Debug, Reflect)]
#[reflect(Resource, Debug)]
pub struct PauseState {
    reasons: HashSet<PauseReason>,
}

impl PauseState {
    /// Adds a pause reason. Returns `true` if the reason was not already held.
    pub fn request(&mut self, reason: impl Into<PauseReason>) -> bool {
        self.reasons.insert(reason.into())
    }

    /// Releases a pause reason. Returns `true` if the reason was held.
    pub fn release(&mut self, reason: impl Into<PauseReason>) -> bool {
        self.reasons.remove(&reason.into())
    }

    /// Requests or releases a reason depending on `paused`.
    pub fn set(&mut self, reason: impl Into<PauseReason>, paused: bool) {
        if paused {
            self.request(reason);
        } else {
            self.release(reason);
        }
    }

    /// Returns `true` if the given reason is held.
    pub fn is_held(&self, reason: &PauseReason) -> bool {
        self.reasons.contains(reason)
    }

    /// Returns `true` if any reason is held.
    pub fn is_paused(&self) -> bool {
        !self.reasons.is_empty()
    }

    /// Iterates the held reasons.
    pub fn reasons(&self) -> impl Iterator<Item = &PauseReason> {
        self.reasons.iter()
    }

    /// Releases every reason.
    pub fn clear(&mut self) {
        self.reasons.clear();
    }
}

/// Run condition that is `true` while gameplay is not paused.
pub fn gameplay_running(pause: Option<Res<PauseState>>) -> bool {
    pause.is_none_or(|pause| !pause.is_paused())
}

/// Run condition that is `true` while gameplay is paused.
pub fn gameplay_paused(pause: Option<Res<PauseState>>) -> bool {
    pause.is_some_and(|pause| pause.is_paused())
}

/// Applies the pause state to virtual time.
///
/// Virtual time is only resumed if this system paused it, so a game that pauses virtual time
/// on its own keeps control over it.
pub fn apply_pause_to_time(
    pause: Res<PauseState>,
    policy: Res<PausePolicy>,
    mut time: ResMut<Time<Virtual>>,
    mut paused_by_kit: Local<bool>,
) {
    if !pause.is_changed() && !policy.is_changed() {
        return;
    }
    let should_pause = policy.pause_virtual_time && pause.is_paused();
    if should_pause && !time.is_paused() {
        time.pause();
        *paused_by_kit = true;
    } else if !should_pause && *paused_by_kit {
        time.unpause();
        *paused_by_kit = false;
    }
}
