//! Evaluation of bindings into action state.

use bevy::input::gamepad::{Gamepad, GamepadAxis};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

use crate::InputSuspension;
use crate::action::{Action, ActionState};
use crate::binding::{Axis2Binding, AxisBinding, Binding, ButtonBinding, GamepadStick, KeyboardAxis};
use crate::context::InputContexts;
use crate::map::{GamepadAssignment, InputMap};
use crate::rebind::Rebinding;
use crate::virtual_controls::VirtualControls;

/// Read-only access to every input device, gathered once per frame.
pub(crate) struct InputSources<'a> {
    pub keys: Option<&'a ButtonInput<KeyCode>>,
    pub mouse: Option<&'a ButtonInput<MouseButton>>,
    pub motion: Vec2,
    pub scroll: Vec2,
    pub gamepads: Vec<&'a Gamepad>,
    pub virtual_controls: &'a VirtualControls,
}

impl InputSources<'_> {
    fn button(&self, binding: &ButtonBinding, threshold: f32) -> (bool, f32) {
        match binding {
            ButtonBinding::Key(key) => {
                let pressed = self.keys.is_some_and(|keys| keys.pressed(*key));
                (pressed, f32::from(u8::from(pressed)))
            }
            ButtonBinding::Mouse(button) => {
                let pressed = self.mouse.is_some_and(|mouse| mouse.pressed(*button));
                (pressed, f32::from(u8::from(pressed)))
            }
            ButtonBinding::Gamepad(button) => {
                let mut value: f32 = 0.0;
                for gamepad in &self.gamepads {
                    let analog = gamepad.get(*button);
                    let digital = f32::from(u8::from(gamepad.pressed(*button)));
                    value = value.max(analog.unwrap_or(digital).max(digital));
                }
                (value >= threshold.min(1.0) && value > 0.0, value)
            }
            ButtonBinding::Virtual(control) => {
                let pressed = self.virtual_controls.button(*control);
                (pressed, f32::from(u8::from(pressed)))
            }
        }
    }

    fn key(&self, key: KeyCode) -> f32 {
        f32::from(u8::from(self.keys.is_some_and(|keys| keys.pressed(key))))
    }

    fn gamepad_axis(&self, axis: GamepadAxis) -> f32 {
        let mut value: f32 = 0.0;
        for gamepad in &self.gamepads {
            let sample = gamepad.get(axis).unwrap_or(0.0);
            if sample.abs() > value.abs() {
                value = sample;
            }
        }
        value
    }

    fn stick(&self, stick: GamepadStick) -> Vec2 {
        let mut value = Vec2::ZERO;
        for gamepad in &self.gamepads {
            let sample = match stick {
                GamepadStick::Left => gamepad.left_stick(),
                GamepadStick::Right => gamepad.right_stick(),
            };
            if sample.length_squared() > value.length_squared() {
                value = sample;
            }
        }
        value
    }

    fn keyboard_axis(&self, axis: &KeyboardAxis) -> Vec2 {
        Vec2::new(
            self.key(axis.right) - self.key(axis.left),
            self.key(axis.up) - self.key(axis.down),
        )
        .normalize_or_zero()
    }
}

/// The contribution of one binding to an action.
#[derive(Default)]
struct Contribution {
    pressed: bool,
    value: f32,
    axis: Vec2,
    unbounded: bool,
}

fn evaluate<A: Action>(binding: &Binding, map: &InputMap<A>, sources: &InputSources) -> Contribution {
    match binding {
        Binding::Button(button) => {
            let (pressed, value) = sources.button(button, map.trigger_threshold);
            Contribution {
                pressed,
                value,
                axis: Vec2::new(value, 0.0),
                unbounded: false,
            }
        }
        Binding::Axis(axis) => {
            let (value, unbounded) = match axis {
                AxisBinding::Gamepad(axis) => {
                    (map.dead_zone.apply_scalar(sources.gamepad_axis(*axis)), false)
                }
                AxisBinding::Buttons { negative, positive } => {
                    let negative = sources.button(negative, map.trigger_threshold).1;
                    let positive = sources.button(positive, map.trigger_threshold).1;
                    (positive - negative, false)
                }
                AxisBinding::MouseWheel { sensitivity } => (sources.scroll.y * sensitivity, true),
                AxisBinding::Virtual(control) => (sources.virtual_controls.axis(*control).x, false),
            };
            Contribution {
                pressed: value != 0.0,
                value,
                axis: Vec2::new(value, 0.0),
                unbounded,
            }
        }
        Binding::Axis2(axis) => {
            let (axis, unbounded) = match axis {
                Axis2Binding::Keyboard(keys) => (sources.keyboard_axis(keys), false),
                Axis2Binding::Stick(stick) => (map.dead_zone.apply(sources.stick(*stick)), false),
                Axis2Binding::MouseMotion { sensitivity } => {
                    (Vec2::new(sources.motion.x, -sources.motion.y) * *sensitivity, true)
                }
                Axis2Binding::MouseWheel { sensitivity } => (sources.scroll * *sensitivity, true),
                Axis2Binding::Virtual(control) => (sources.virtual_controls.axis(*control), false),
            };
            Contribution {
                pressed: axis != Vec2::ZERO,
                value: axis.length(),
                axis,
                unbounded,
            }
        }
    }
}

/// Recomputes [`ActionState<A>`] from the active contexts of [`InputMap<A>`].
pub(crate) fn update_action_state<A: Action>(
    mut state: ResMut<ActionState<A>>,
    map: Res<InputMap<A>>,
    contexts: Res<InputContexts>,
    suspension: Res<InputSuspension>,
    rebinding: Option<Res<Rebinding<A>>>,
    time: Res<Time<Real>>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    motion: Option<Res<AccumulatedMouseMotion>>,
    scroll: Option<Res<AccumulatedMouseScroll>>,
    virtual_controls: Res<VirtualControls>,
    gamepads: Query<(Entity, &Gamepad)>,
) {
    state.begin_update();

    let capturing = rebinding.is_some_and(|rebinding| rebinding.is_blocking());
    if suspension.is_suspended() || capturing {
        state.finish_update(time.delta());
        return;
    }

    let sources = InputSources {
        keys: keys.as_deref(),
        mouse: mouse.as_deref(),
        motion: motion.map(|motion| motion.delta).unwrap_or_default(),
        scroll: scroll.map(|scroll| scroll.delta).unwrap_or_default(),
        gamepads: gamepads
            .iter()
            .filter(|(entity, _)| match map.gamepad {
                GamepadAssignment::Any => true,
                GamepadAssignment::Only(only) => *entity == only,
                GamepadAssignment::None => false,
            })
            .map(|(_, gamepad)| gamepad)
            .collect(),
        virtual_controls: &virtual_controls,
    };

    for (context, bindings) in map.context_bindings() {
        if !contexts.is_active(context) {
            continue;
        }
        for (action, binding) in bindings {
            let contribution = evaluate(binding, &map, &sources);
            let data = state.data_mut(*action);
            data.pressed |= contribution.pressed;
            if contribution.value.abs() > data.value.abs() {
                data.value = contribution.value;
            }
            // Bounded sources (keys, sticks) combine up to unit length; unbounded sources
            // (mouse motion, wheel) pass through so their magnitude is preserved.
            let combined = data.axis + contribution.axis;
            data.axis = if contribution.unbounded {
                combined
            } else {
                combined.clamp_length_max(data.axis.length().max(1.0))
            };
        }
    }

    state.finish_update(time.delta());
}
