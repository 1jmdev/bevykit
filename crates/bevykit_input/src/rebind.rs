//! Interactive rebinding: capture the next input the player presses.
//!
//! While a rebind is in progress, actions of that type read as released so the captured
//! press does not also trigger gameplay.
//!
//! ```ignore
//! fn start(mut rebinding: ResMut<Rebinding<GameAction>>) {
//!     rebinding.start(InputContext::Gameplay, GameAction::Interact, 0);
//! }
//!
//! fn report(mut finished: MessageReader<RebindFinished<GameAction>>) {
//!     for result in finished.read() {
//!         if !result.conflicts.is_empty() {
//!             // Tell the player which actions now share the input.
//!         }
//!     }
//! }
//! ```

use bevy::input::gamepad::{Gamepad, GamepadButton};
use bevy::prelude::*;

use crate::action::Action;
use crate::binding::{Binding, ButtonBinding};
use crate::context::InputContext;
use crate::map::InputMap;

#[derive(Clone, Debug)]
struct PendingRebind<A: Action> {
    context: InputContext,
    action: A,
    slot: usize,
}

/// Controls interactive rebinding for actions of type `A`.
#[derive(Resource, Debug)]
pub struct Rebinding<A: Action> {
    pending: Option<PendingRebind<A>>,
    awaiting_release: Option<ButtonBinding>,
    /// Inputs that cancel the rebind instead of being captured.
    pub cancel_inputs: Vec<ButtonBinding>,
    /// Whether mouse buttons can be captured.
    pub accept_mouse: bool,
}

impl<A: Action> Default for Rebinding<A> {
    fn default() -> Self {
        Self {
            pending: None,
            awaiting_release: None,
            cancel_inputs: vec![
                ButtonBinding::Key(KeyCode::Escape),
                ButtonBinding::Gamepad(GamepadButton::Start),
            ],
            accept_mouse: true,
        }
    }
}

impl<A: Action> Rebinding<A> {
    /// Begins capturing a new binding for `slot` of `action` in `context`.
    pub fn start(&mut self, context: impl Into<InputContext>, action: A, slot: usize) {
        self.pending = Some(PendingRebind {
            context: context.into(),
            action,
            slot,
        });
    }

    /// Abandons the rebind in progress.
    pub fn cancel(&mut self) {
        self.pending = None;
    }

    /// Returns `true` while waiting for an input.
    pub fn is_active(&self) -> bool {
        self.pending.is_some()
    }

    /// Returns `true` while actions should read as released: during capture and until the
    /// captured input is released.
    pub fn is_blocking(&self) -> bool {
        self.pending.is_some() || self.awaiting_release.is_some()
    }

    /// Returns the action being rebound.
    pub fn action(&self) -> Option<A> {
        self.pending.as_ref().map(|pending| pending.action)
    }
}

/// Sent when a rebind completes or is cancelled.
#[derive(Message, Clone, Debug)]
pub struct RebindFinished<A: Action> {
    /// The context of the rebound action.
    pub context: InputContext,
    /// The rebound action.
    pub action: A,
    /// The captured binding, or `None` if the rebind was cancelled.
    pub binding: Option<Binding>,
    /// Other actions in the same context that read the captured input.
    pub conflicts: Vec<A>,
}

pub(crate) fn capture_rebind<A: Action>(
    mut rebinding: ResMut<Rebinding<A>>,
    mut map: ResMut<InputMap<A>>,
    mut finished: MessageWriter<RebindFinished<A>>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    gamepads: Query<&Gamepad>,
) {
    if let Some(waiting) = rebinding.awaiting_release {
        let held = match waiting {
            ButtonBinding::Key(key) => keys.as_deref().is_some_and(|keys| keys.pressed(key)),
            ButtonBinding::Mouse(button) => {
                mouse.as_deref().is_some_and(|mouse| mouse.pressed(button))
            }
            ButtonBinding::Gamepad(button) => gamepads.iter().any(|pad| pad.pressed(button)),
            ButtonBinding::Virtual(_) => false,
        };
        if !held {
            rebinding.awaiting_release = None;
        }
    }

    let Some(pending) = rebinding.pending.clone() else {
        return;
    };

    let captured = keys
        .as_deref()
        .and_then(|keys| keys.get_just_pressed().next().copied().map(ButtonBinding::Key))
        .or_else(|| {
            mouse
                .as_deref()
                .filter(|_| rebinding.accept_mouse)
                .and_then(|mouse| mouse.get_just_pressed().next().copied())
                .map(ButtonBinding::Mouse)
        })
        .or_else(|| {
            gamepads.iter().find_map(|gamepad| {
                gamepad
                    .digital()
                    .get_just_pressed()
                    .next()
                    .copied()
                    .map(ButtonBinding::Gamepad)
            })
        });

    let Some(captured) = captured else {
        return;
    };
    rebinding.pending = None;
    rebinding.awaiting_release = Some(captured);

    if rebinding.cancel_inputs.contains(&captured) {
        finished.write(RebindFinished {
            context: pending.context,
            action: pending.action,
            binding: None,
            conflicts: Vec::new(),
        });
        return;
    }

    let binding = Binding::Button(captured);
    map.rebind(&pending.context, pending.action, pending.slot, binding);
    let conflicts = map.conflicts_with(&pending.context, pending.action, &binding);
    finished.write(RebindFinished {
        context: pending.context,
        action: pending.action,
        binding: Some(binding),
        conflicts,
    });
}
