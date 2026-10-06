//! Keyboard and controller actions that drive widgets.
//!
//! Widgets read [`UiAction`] instead of raw keys, so players can rebind menu navigation like any
//! other action. The bindings live in the [`InputContext`] returned by [`ui_context`], which is
//! a base context: modal panels block gameplay but leave it active.

use bevy::input::gamepad::GamepadButton;
use bevy::prelude::*;
use bevykit_input::prelude::*;

/// Navigation and activation actions used by widgets.
#[derive(KitAction, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum UiAction {
    /// Move focus up, or increase a vertical value.
    NavigateUp,
    /// Move focus down.
    NavigateDown,
    /// Move focus left, or decrease a value.
    NavigateLeft,
    /// Move focus right, or increase a value.
    NavigateRight,
    /// Activate the focused widget.
    Confirm,
    /// Close the topmost dismissible panel.
    Cancel,
    /// Select the next tab.
    NextTab,
    /// Select the previous tab.
    PreviousTab,
}

/// The input context holding the [`UiAction`] bindings.
pub fn ui_context() -> InputContext {
    InputContext::from_static("ui")
}

/// Installs the default bindings: arrow keys, Enter, Space, Escape, Tab, and the gamepad
/// d-pad, left stick, face buttons, and shoulders.
pub(crate) fn install_default_bindings(
    mut map: ResMut<InputMap<UiAction>>,
    mut contexts: ResMut<InputContexts>,
) {
    contexts.add_base(ui_context());
    let has_bindings = map.iter().next().is_some();
    if has_bindings {
        return;
    }
    map.context(ui_context())
        .button(UiAction::NavigateUp, KeyCode::ArrowUp)
        .button(UiAction::NavigateUp, GamepadButton::DPadUp)
        .button(UiAction::NavigateDown, KeyCode::ArrowDown)
        .button(UiAction::NavigateDown, GamepadButton::DPadDown)
        .button(UiAction::NavigateLeft, KeyCode::ArrowLeft)
        .button(UiAction::NavigateLeft, GamepadButton::DPadLeft)
        .button(UiAction::NavigateRight, KeyCode::ArrowRight)
        .button(UiAction::NavigateRight, GamepadButton::DPadRight)
        .button(UiAction::Confirm, KeyCode::Enter)
        .button(UiAction::Confirm, KeyCode::NumpadEnter)
        .button(UiAction::Confirm, KeyCode::Space)
        .button(UiAction::Confirm, GamepadButton::South)
        .button(UiAction::Cancel, KeyCode::Escape)
        .button(UiAction::Cancel, GamepadButton::East)
        .button(UiAction::NextTab, GamepadButton::RightTrigger)
        .button(UiAction::PreviousTab, GamepadButton::LeftTrigger);
}

/// Emits a repeating navigation pulse while a direction is held, like a keyboard's key repeat.
#[derive(Resource, Debug, Clone)]
pub struct NavigationRepeat {
    /// Seconds before the first repeat.
    pub delay: f32,
    /// Seconds between repeats.
    pub interval: f32,
    held: f32,
    next: f32,
}

impl Default for NavigationRepeat {
    fn default() -> Self {
        Self {
            delay: 0.4,
            interval: 0.08,
            held: 0.0,
            next: 0.0,
        }
    }
}

/// The direction requested this frame by navigation actions or stick deflection.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct NavigationRequest(pub Option<Dir2>);

pub(crate) fn update_navigation_request(
    actions: Res<ActionState<UiAction>>,
    gamepads: Query<&bevy::input::gamepad::Gamepad>,
    time: Res<Time<Real>>,
    mut repeat: ResMut<NavigationRepeat>,
    mut request: ResMut<NavigationRequest>,
) {
    let mut direction = Vec2::ZERO;
    for (action, vector) in [
        (UiAction::NavigateUp, Vec2::Y),
        (UiAction::NavigateDown, Vec2::NEG_Y),
        (UiAction::NavigateLeft, Vec2::NEG_X),
        (UiAction::NavigateRight, Vec2::X),
    ] {
        if actions.pressed(action) {
            direction += vector;
        }
    }
    if direction == Vec2::ZERO {
        for gamepad in &gamepads {
            let stick = gamepad.left_stick();
            if stick.length() > 0.6 {
                direction = if stick.x.abs() > stick.y.abs() {
                    Vec2::new(stick.x.signum(), 0.0)
                } else {
                    Vec2::new(0.0, stick.y.signum())
                };
                break;
            }
        }
    }

    let held_direction = Dir2::new(direction).ok();
    request.0 = None;
    let Some(held_direction) = held_direction else {
        repeat.held = 0.0;
        return;
    };
    if repeat.held == 0.0 {
        request.0 = Some(held_direction);
        repeat.next = repeat.delay;
    }
    repeat.held += time.delta_secs();
    if repeat.held >= repeat.next {
        request.0 = Some(held_direction);
        repeat.next += repeat.interval;
    }
}
