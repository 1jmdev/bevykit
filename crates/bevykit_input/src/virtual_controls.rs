//! Values written by on-screen controls and read by virtual bindings.
//!
//! Touch joysticks and buttons emit the same actions as physical controls: the control writes
//! its value here, and a [`VirtualControl`] binding feeds that value into the action map. The
//! game supplies every visual; [`VirtualButton`] and [`VirtualStick`] only implement behavior.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::binding::VirtualControl;
use crate::pointer::{PointerPhase, PointerRouter};

/// Current values of every virtual control.
#[derive(Resource, Default, Debug)]
pub struct VirtualControls {
    buttons: HashMap<VirtualControl, bool>,
    axes: HashMap<VirtualControl, Vec2>,
}

impl VirtualControls {
    /// Sets the state of a virtual button.
    pub fn set_button(&mut self, control: VirtualControl, pressed: bool) {
        self.buttons.insert(control, pressed);
    }

    /// Sets the value of a virtual axis or stick. Components are clamped to `-1.0..=1.0`.
    pub fn set_axis(&mut self, control: VirtualControl, value: Vec2) {
        self.axes
            .insert(control, value.clamp(Vec2::NEG_ONE, Vec2::ONE));
    }

    /// Returns `true` if the virtual button is held.
    pub fn button(&self, control: VirtualControl) -> bool {
        self.buttons.get(&control).copied().unwrap_or(false)
    }

    /// Returns the value of a virtual axis or stick.
    pub fn axis(&self, control: VirtualControl) -> Vec2 {
        self.axes.get(&control).copied().unwrap_or(Vec2::ZERO)
    }

    /// Releases every button and centers every axis.
    pub fn reset(&mut self) {
        self.buttons.clear();
        self.axes.clear();
    }
}

/// Makes a UI node act as a virtual button while a pointer holds it.
#[derive(Component, Clone, Copy, Debug)]
pub struct VirtualButton(pub VirtualControl);

/// Makes a UI node act as a virtual joystick.
///
/// The stick captures the pointer that pressed it and reports the offset from the press
/// position, divided by `radius` and clamped to the unit circle. Position the game's knob
/// visual from [`VirtualStick::value`].
#[derive(Component, Clone, Copy, Debug)]
pub struct VirtualStick {
    /// The control the stick writes.
    pub control: VirtualControl,
    /// The offset, in logical pixels, that reads as full deflection.
    pub radius: f32,
    value: Vec2,
    origin: Option<Vec2>,
}

impl VirtualStick {
    /// Creates a stick with the given control and radius.
    pub fn new(control: VirtualControl, radius: f32) -> Self {
        Self {
            control,
            radius,
            value: Vec2::ZERO,
            origin: None,
        }
    }

    /// Returns the current deflection, with positive Y pointing up.
    pub fn value(&self) -> Vec2 {
        self.value
    }

    /// Returns the screen position where the current drag began.
    pub fn origin(&self) -> Option<Vec2> {
        self.origin
    }
}

pub(crate) fn update_virtual_buttons(
    mut controls: ResMut<VirtualControls>,
    buttons: Query<(Entity, &VirtualButton)>,
    router: Res<PointerRouter>,
) {
    for (entity, button) in &buttons {
        let held = router
            .pointers()
            .any(|pointer| pointer.is_down() && pointer.press_targets().contains(&entity));
        controls.set_button(button.0, held);
    }
}

pub(crate) fn update_virtual_sticks(
    mut controls: ResMut<VirtualControls>,
    mut sticks: Query<(Entity, &mut VirtualStick)>,
    mut router: ResMut<PointerRouter>,
) {
    for (entity, mut stick) in &mut sticks {
        if let Some(press) = router.press_on(entity) {
            router.capture(press.id, entity);
            stick.origin = Some(press.position);
        }
        let captured = router
            .captured_by(entity)
            .find(|pointer| pointer.phase() != PointerPhase::Released);
        let value = match (captured, stick.origin) {
            (Some(pointer), Some(origin)) => {
                let offset = (pointer.position() - origin) / stick.radius.max(1.0);
                Vec2::new(offset.x, -offset.y).clamp_length_max(1.0)
            }
            _ => {
                stick.origin = None;
                Vec2::ZERO
            }
        };
        if stick.value != value {
            stick.value = value;
        }
        controls.set_axis(stick.control, value);
    }
}
