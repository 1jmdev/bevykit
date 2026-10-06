//! Physical and virtual input sources that drive actions.

use bevy::input::gamepad::{GamepadAxis, GamepadButton};
use bevy::prelude::*;
use bevykit_core::key::{IntoKey, Key};
use serde::{Deserialize, Serialize};

/// Identifies an on-screen virtual control, such as a touch joystick or button.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Reflect)]
pub struct VirtualControl(pub Key);

impl VirtualControl {
    /// Creates an identifier from a string literal in a `const` context.
    pub const fn from_static(name: &'static str) -> Self {
        Self(Key::from_static(name))
    }

    /// Creates an identifier from any key-convertible value.
    pub fn new(id: impl IntoKey) -> Self {
        Self(id.into_key())
    }
}

/// A digital input: a key, a mouse button, a gamepad button, or a virtual button.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Reflect, Serialize, Deserialize)]
pub enum ButtonBinding {
    /// A keyboard key, identified by its physical position.
    Key(KeyCode),
    /// A mouse button.
    Mouse(MouseButton),
    /// A gamepad button. Analog buttons count as pressed above the map's trigger threshold.
    Gamepad(GamepadButton),
    /// An on-screen virtual button.
    #[serde(skip)]
    Virtual(VirtualControl),
}

impl From<KeyCode> for ButtonBinding {
    fn from(key: KeyCode) -> Self {
        Self::Key(key)
    }
}

impl From<MouseButton> for ButtonBinding {
    fn from(button: MouseButton) -> Self {
        Self::Mouse(button)
    }
}

impl From<GamepadButton> for ButtonBinding {
    fn from(button: GamepadButton) -> Self {
        Self::Gamepad(button)
    }
}

impl From<VirtualControl> for ButtonBinding {
    fn from(control: VirtualControl) -> Self {
        Self::Virtual(control)
    }
}

/// A one-dimensional input with values in `-1.0..=1.0`.
#[derive(Clone, Copy, PartialEq, Debug, Reflect, Serialize, Deserialize)]
pub enum AxisBinding {
    /// A gamepad axis, such as a single stick direction or a trigger.
    Gamepad(GamepadAxis),
    /// Two buttons forming an axis: `negative` yields `-1.0` and `positive` yields `1.0`.
    Buttons {
        /// The button producing negative values.
        negative: ButtonBinding,
        /// The button producing positive values.
        positive: ButtonBinding,
    },
    /// Vertical mouse wheel movement, scaled by `sensitivity`.
    MouseWheel {
        /// Multiplier applied to the scroll delta.
        sensitivity: f32,
    },
    /// An on-screen virtual axis.
    #[serde(skip)]
    Virtual(VirtualControl),
}

impl From<GamepadAxis> for AxisBinding {
    fn from(axis: GamepadAxis) -> Self {
        Self::Gamepad(axis)
    }
}

/// A gamepad thumbstick.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Reflect, Serialize, Deserialize)]
pub enum GamepadStick {
    /// The left thumbstick.
    Left,
    /// The right thumbstick.
    Right,
}

/// Four keys forming a two-dimensional axis.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Reflect, Serialize, Deserialize)]
pub struct KeyboardAxis {
    /// Key producing positive Y.
    pub up: KeyCode,
    /// Key producing negative Y.
    pub down: KeyCode,
    /// Key producing negative X.
    pub left: KeyCode,
    /// Key producing positive X.
    pub right: KeyCode,
}

impl KeyboardAxis {
    /// The W, A, S, and D keys.
    pub const fn wasd() -> Self {
        Self {
            up: KeyCode::KeyW,
            down: KeyCode::KeyS,
            left: KeyCode::KeyA,
            right: KeyCode::KeyD,
        }
    }

    /// The arrow keys.
    pub const fn arrows() -> Self {
        Self {
            up: KeyCode::ArrowUp,
            down: KeyCode::ArrowDown,
            left: KeyCode::ArrowLeft,
            right: KeyCode::ArrowRight,
        }
    }
}

/// A two-dimensional input. Positive Y points up.
#[derive(Clone, Copy, PartialEq, Debug, Reflect, Serialize, Deserialize)]
pub enum Axis2Binding {
    /// Four keys. Diagonals are normalized.
    Keyboard(KeyboardAxis),
    /// A gamepad thumbstick, with the map's dead zone applied.
    Stick(GamepadStick),
    /// Mouse movement in logical pixels, scaled by `sensitivity`. Not clamped.
    MouseMotion {
        /// Multiplier applied to the motion delta.
        sensitivity: f32,
    },
    /// Mouse wheel movement, scaled by `sensitivity`. Not clamped.
    MouseWheel {
        /// Multiplier applied to the scroll delta.
        sensitivity: f32,
    },
    /// An on-screen virtual joystick.
    #[serde(skip)]
    Virtual(VirtualControl),
}

impl From<KeyboardAxis> for Axis2Binding {
    fn from(axis: KeyboardAxis) -> Self {
        Self::Keyboard(axis)
    }
}

impl From<GamepadStick> for Axis2Binding {
    fn from(stick: GamepadStick) -> Self {
        Self::Stick(stick)
    }
}

impl From<VirtualControl> for Axis2Binding {
    fn from(control: VirtualControl) -> Self {
        Self::Virtual(control)
    }
}

/// Any input that can drive an action.
#[derive(Clone, Copy, PartialEq, Debug, Reflect, Serialize, Deserialize)]
pub enum Binding {
    /// A digital input.
    Button(ButtonBinding),
    /// A one-dimensional input.
    Axis(AxisBinding),
    /// A two-dimensional input.
    Axis2(Axis2Binding),
}

impl Binding {
    /// Returns a short, human-readable description suitable for prompts and binding lists.
    ///
    /// Games that localize binding names can match on the binding themselves instead.
    pub fn describe(&self) -> String {
        match self {
            Binding::Button(button) => describe_button(button),
            Binding::Axis(AxisBinding::Gamepad(axis)) => format!("{axis:?}"),
            Binding::Axis(AxisBinding::Buttons { negative, positive }) => {
                format!("{} / {}", describe_button(negative), describe_button(positive))
            }
            Binding::Axis(AxisBinding::MouseWheel { .. }) => "Mouse Wheel".to_string(),
            Binding::Axis(AxisBinding::Virtual(_)) => "Virtual Axis".to_string(),
            Binding::Axis2(Axis2Binding::Keyboard(axis)) => {
                if *axis == KeyboardAxis::wasd() {
                    "WASD".to_string()
                } else if *axis == KeyboardAxis::arrows() {
                    "Arrow Keys".to_string()
                } else {
                    [axis.up, axis.left, axis.down, axis.right]
                        .iter()
                        .map(|key| describe_key(*key))
                        .collect::<Vec<_>>()
                        .join("")
                }
            }
            Binding::Axis2(Axis2Binding::Stick(GamepadStick::Left)) => "Left Stick".to_string(),
            Binding::Axis2(Axis2Binding::Stick(GamepadStick::Right)) => "Right Stick".to_string(),
            Binding::Axis2(Axis2Binding::MouseMotion { .. }) => "Mouse".to_string(),
            Binding::Axis2(Axis2Binding::MouseWheel { .. }) => "Mouse Wheel".to_string(),
            Binding::Axis2(Axis2Binding::Virtual(_)) => "Virtual Stick".to_string(),
        }
    }

    /// Returns `true` if both bindings read the same physical input.
    pub fn overlaps(&self, other: &Binding) -> bool {
        match (self, other) {
            (Binding::Button(a), Binding::Button(b)) => a == b,
            (Binding::Axis2(Axis2Binding::Keyboard(a)), Binding::Button(ButtonBinding::Key(key)))
            | (Binding::Button(ButtonBinding::Key(key)), Binding::Axis2(Axis2Binding::Keyboard(a))) => {
                [a.up, a.down, a.left, a.right].contains(key)
            }
            _ => self == other,
        }
    }
}

impl From<ButtonBinding> for Binding {
    fn from(binding: ButtonBinding) -> Self {
        Self::Button(binding)
    }
}

impl From<AxisBinding> for Binding {
    fn from(binding: AxisBinding) -> Self {
        Self::Axis(binding)
    }
}

impl From<Axis2Binding> for Binding {
    fn from(binding: Axis2Binding) -> Self {
        Self::Axis2(binding)
    }
}

fn describe_button(button: &ButtonBinding) -> String {
    match button {
        ButtonBinding::Key(key) => describe_key(*key),
        ButtonBinding::Mouse(MouseButton::Left) => "Left Click".to_string(),
        ButtonBinding::Mouse(MouseButton::Right) => "Right Click".to_string(),
        ButtonBinding::Mouse(MouseButton::Middle) => "Middle Click".to_string(),
        ButtonBinding::Mouse(other) => format!("Mouse {other:?}"),
        ButtonBinding::Gamepad(button) => format!("Gamepad {button:?}"),
        ButtonBinding::Virtual(_) => "Virtual Button".to_string(),
    }
}

/// Returns a readable name for a key, such as `E`, `1`, or `Space`.
pub fn describe_key(key: KeyCode) -> String {
    let debug = format!("{key:?}");
    if let Some(letter) = debug.strip_prefix("Key") {
        return letter.to_string();
    }
    if let Some(digit) = debug.strip_prefix("Digit") {
        return digit.to_string();
    }
    match key {
        KeyCode::Escape => "Esc".to_string(),
        KeyCode::ArrowUp => "Up".to_string(),
        KeyCode::ArrowDown => "Down".to_string(),
        KeyCode::ArrowLeft => "Left".to_string(),
        KeyCode::ArrowRight => "Right".to_string(),
        KeyCode::ShiftLeft | KeyCode::ShiftRight => "Shift".to_string(),
        KeyCode::ControlLeft | KeyCode::ControlRight => "Ctrl".to_string(),
        KeyCode::AltLeft | KeyCode::AltRight => "Alt".to_string(),
        _ => debug,
    }
}
